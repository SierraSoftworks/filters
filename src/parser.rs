use std::iter::Peekable;

use human_errors::{Error, ResultExt};

use super::{FilterValue, expr::Expr, location::Loc, pattern::Glob, token::Token};

pub struct Parser<'a, I: Iterator<Item = Result<Token<'a>, Error>>> {
    tokens: Peekable<I>,
}

impl<'a, I: Iterator<Item = Result<Token<'a>, Error>>> Parser<'a, I> {
    pub fn parse(tokens: I) -> Result<Expr<'a>, Error> {
        let mut parser = Parser {
            tokens: tokens.peekable(),
        };

        let expr = parser.or()?;
        parser.ensure_end()?;

        Ok(expr)
    }

    fn ensure_end(&mut self) -> Result<(), Error> {
        if let Some(result) = self.tokens.next() {
            let token = result?;
            Err(human_errors::user(
                format!(
                    "Your filter expression contained an unexpected '{}' at {}.",
                    token,
                    token.location(),
                ),
                &["Make sure that you have written a valid filter query."],
            ))
        } else {
            Ok(())
        }
    }

    fn or(&mut self) -> Result<Expr<'a>, Error> {
        let mut expr = self.and()?;

        while matches!(self.tokens.peek(), Some(Ok(Token::Or(..)))) {
            let token = self.tokens.next().unwrap()?;
            let right = self.and()?;
            expr = Expr::Logical(Box::new(expr), token, Box::new(right));
        }

        Ok(expr)
    }

    fn and(&mut self) -> Result<Expr<'a>, Error> {
        let mut expr = self.equality()?;

        while matches!(self.tokens.peek(), Some(Ok(Token::And(..)))) {
            let token = self.tokens.next().unwrap()?;
            let right = self.equality()?;
            expr = Expr::Logical(Box::new(expr), token, Box::new(right));
        }

        Ok(expr)
    }

    fn equality(&mut self) -> Result<Expr<'a>, Error> {
        let mut expr = self.comparison()?;

        if matches!(
            self.tokens.peek(),
            Some(Ok(Token::Equals(..)) | Ok(Token::NotEquals(..)))
        ) {
            let token = self.tokens.next().unwrap()?;
            let right = self.comparison()?;
            expr = Expr::Binary(Box::new(expr), token, Box::new(right));
        }

        Ok(expr)
    }

    fn comparison(&mut self) -> Result<Expr<'a>, Error> {
        let mut expr = self.term()?;

        if matches!(
            self.tokens.peek(),
            Some(Ok(Token::In(..)))
                | Some(Ok(Token::InCs(..)))
                | Some(Ok(Token::Contains(..)))
                | Some(Ok(Token::ContainsCs(..)))
                | Some(Ok(Token::StartsWith(..)))
                | Some(Ok(Token::StartsWithCs(..)))
                | Some(Ok(Token::EndsWith(..)))
                | Some(Ok(Token::EndsWithCs(..)))
                | Some(Ok(Token::GreaterThan(..)))
                | Some(Ok(Token::GreaterEqual(..)))
                | Some(Ok(Token::SmallerThan(..)))
                | Some(Ok(Token::SmallerEqual(..)))
        ) {
            let token = self.tokens.next().unwrap()?;
            let right = self.unary()?;
            expr = Expr::Binary(Box::new(expr), token, Box::new(right));
        } else if matches!(
            self.tokens.peek(),
            Some(Ok(Token::Like(..))) | Some(Ok(Token::LikeCs(..)))
        ) {
            let token = self.tokens.next().unwrap()?;
            let case_sensitive = matches!(token, Token::LikeCs(..));
            let example = if case_sensitive {
                "branch.name like_cs \"feat/*\""
            } else {
                "branch.name like \"feat/*\""
            };
            let pattern = self.pattern_literal(&token, example)?;
            let glob = if case_sensitive {
                Glob::compile_cs(&pattern)
            } else {
                Glob::compile(&pattern)
            };
            expr = Expr::Like(Box::new(expr), glob);
        } else if matches!(self.tokens.peek(), Some(Ok(Token::Matches(..)))) {
            let token = self.tokens.next().unwrap()?;

            #[cfg(not(feature = "regex"))]
            return Err(human_errors::user(
                format!(
                    "Your filter uses the 'matches' operator at {}, but this build of the filters crate does not include regular expression support.",
                    token.location()
                ),
                &[
                    "Enable the 'regex' feature of the filters crate to use the 'matches' operator (e.g. filters = { version = \"0.1\", features = [\"regex\"] }).",
                    "Alternatively, use the 'like' operator for simple glob-style patterns (e.g. branch.name like \"feat/*\").",
                ],
            ));

            #[cfg(feature = "regex")]
            {
                let pattern = self.pattern_literal(&token, "branch.name matches r\"^feat/.+$\"")?;
                let regex = crate::pattern::CompiledRegex::compile(&pattern).wrap_user_err(
                    format!(
                        "Failed to compile the regular expression pattern \"{pattern}\" for the 'matches' operator at {}.",
                        token.location()
                    ),
                    &[
                        "Make sure that your pattern is a valid regular expression as understood by the Rust regex crate (https://docs.rs/regex).",
                        "Remember that within a normal \"...\" string you need to escape backslashes (\\\\d); raw strings (r\"^v\\d+$\") avoid this.",
                    ],
                )?;
                expr = Expr::Matches(Box::new(expr), regex);
            }
        }

        Ok(expr)
    }

    fn term(&mut self) -> Result<Expr<'a>, Error> {
        let mut expr = self.unary()?;

        while matches!(
            self.tokens.peek(),
            Some(Ok(Token::Plus(..)) | Ok(Token::Minus(..)))
        ) {
            let token = self.tokens.next().unwrap()?;
            let right = self.unary()?;
            expr = Expr::Binary(Box::new(expr), token, Box::new(right));
        }

        Ok(expr)
    }

    /// Reads the string literal which provides the pattern for a `like` or
    /// `matches` operator, processing escape sequences for plain strings and
    /// taking raw strings verbatim. Any other token is a parse error: patterns
    /// must be available at parse time so that they can be pre-compiled.
    fn pattern_literal(&mut self, operator: &Token, example: &str) -> Result<String, Error> {
        match self.tokens.next() {
            Some(Ok(Token::String(.., s))) => Ok(s.replace("\\\"", "\"").replace("\\\\", "\\")),
            Some(Ok(Token::RawString(.., s))) => Ok(s.to_string()),
            Some(Ok(token)) => Err(human_errors::user(
                format!(
                    "The '{}' operator at {} must be followed by its pattern as a string literal, but we found '{}' at {} instead (for example: {}).",
                    operator.lexeme(),
                    operator.location(),
                    token,
                    token.location(),
                    example,
                ),
                &[
                    "Provide the pattern as a string literal, since patterns are compiled when the filter is parsed and cannot be computed from properties.",
                ],
            )),
            Some(Err(err)) => Err(err),
            None => Err(human_errors::user(
                format!(
                    "We reached the end of your filter expression while looking for the string pattern of the '{}' operator at {} (for example: {}).",
                    operator.lexeme(),
                    operator.location(),
                    example,
                ),
                &[
                    "Make sure that you have written a valid filter query and that you haven't forgotten part of it.",
                ],
            )),
        }
    }

    fn unary(&mut self) -> Result<Expr<'a>, Error> {
        if matches!(self.tokens.peek(), Some(Ok(Token::Not(..)))) {
            let token = self.tokens.next().unwrap()?;
            let right = self.unary()?;
            Ok(Expr::Unary(token, Box::new(right)))
        } else {
            self.primary()
        }
    }

    fn primary(&mut self) -> Result<Expr<'a>, Error> {
        match self.tokens.peek() {
            Some(Ok(Token::LeftParen(..))) => {
                let start = self.tokens.next().unwrap()?;
                let expr = self.or()?;
                if let Some(Ok(Token::RightParen(..))) = self.tokens.next() {
                    Ok(expr)
                } else {
                    Err(human_errors::user(
                        format!(
                            "When attempting to parse a grouped filter expression starting at {}, we didn't find the closing ')' where we expected to.",
                            start.location()
                        ),
                        &["Make sure that you have balanced your parentheses correctly."],
                    ))
                }
            }
            Some(Ok(Token::LeftBracket(..))) => {
                let start = self.tokens.next().unwrap()?;
                let mut items = Vec::new();
                while !matches!(self.tokens.peek(), Some(Ok(Token::RightBracket(..)))) {
                    items.push(self.literal()?);
                    if matches!(self.tokens.peek(), Some(Ok(Token::Comma(..)))) {
                        self.tokens.next();
                    } else {
                        break;
                    }
                }

                if let Some(Ok(Token::RightBracket(..))) = self.tokens.next() {
                    Ok(Expr::Literal(items.into()))
                } else {
                    Err(human_errors::user(
                        format!(
                            "When attempting to parse a list filter expression starting at {}, we didn't find the closing ']' where we expected to.",
                            start.location()
                        ),
                        &["Make sure that you have closed your tuple brackets correctly."],
                    ))
                }
            }
            Some(Ok(Token::Property(..))) => {
                if let Some(Ok(Token::Property(loc, p))) = self.tokens.next() {
                    if matches!(self.tokens.peek(), Some(Ok(Token::LeftParen(..)))) {
                        self.tokens.next();
                        self.function_call(loc, p)
                    } else {
                        Ok(Expr::Property(p))
                    }
                } else {
                    unreachable!()
                }
            }
            Some(Ok(..)) => self.literal().map(Expr::Literal),
            Some(Err(..)) => Err(self.tokens.next().unwrap().unwrap_err()),
            None => Err(human_errors::user(
                "We reached the end of your filter expression while waiting for a [true, false, \"string\", number, (group), or property.name].",
                &[
                    "Make sure that you have written a valid filter query and that you haven't forgotten part of it.",
                ],
            )),
        }
    }

    fn function_call(&mut self, loc: Loc, name: &'a str) -> Result<Expr<'a>, Error> {
        let mut args = Vec::new();
        if !matches!(self.tokens.peek(), Some(Ok(Token::RightParen(..)))) {
            loop {
                args.push(self.or()?);
                if matches!(self.tokens.peek(), Some(Ok(Token::Comma(..)))) {
                    self.tokens.next();
                } else {
                    break;
                }
            }
        }

        match self.tokens.next() {
            Some(Ok(Token::RightParen(..))) => {}
            Some(Err(err)) => return Err(err),
            _ => {
                return Err(human_errors::user(
                    format!(
                        "When attempting to parse the arguments to the '{name}()' function call at {loc}, we didn't find the closing ')' where we expected to."
                    ),
                    &["Make sure that you have closed your function call's parentheses correctly."],
                ));
            }
        }

        Self::check_function(name, loc, args.len())?;
        Ok(Expr::FunctionCall(name, args))
    }

    /// Validates a function call at parse time, ensuring that the function is
    /// known (and usable with the current crate features) and that it has been
    /// called with the correct number of arguments.
    fn check_function(name: &str, loc: Loc, arity: usize) -> Result<(), Error> {
        match name {
            "now" => {
                let _ = arity;
                Err(human_errors::user(
                    format!(
                        "Your filter called the 'now()' function at {loc}, but datetime support is not enabled in this build."
                    ),
                    &[
                        "Enable the 'chrono' feature of the filters crate to use datetime functions like 'now()'.",
                    ],
                ))
            }
            _ => Err(human_errors::user(
                format!("Your filter called an unknown function '{name}()' at {loc}."),
                &[
                    "Make sure that you are calling one of the functions supported by the filter language: now().",
                ],
            )),
        }
    }

    fn literal(&mut self) -> Result<FilterValue, Error> {
        match self.tokens.next() {
            Some(Ok(Token::True(..))) => Ok(true.into()),
            Some(Ok(Token::False(..))) => Ok(false.into()),
            Some(Ok(Token::Number(loc, n))) => Ok(FilterValue::Number(n.parse().wrap_user_err(
                format!("Failed to parse the number '{n}' which you provided at {}.", loc),
                &["Please make sure that the number is well formatted. It should be in the form 123, or 123.45."],
            )?)),
            Some(Ok(Token::String(.., s))) => Ok(if s.contains('\\') {
                s.replace("\\\"", "\"").replace("\\\\", "\\").into()
            } else {
                // Fast path: strings without escape sequences don't need the
                // (allocating) replacement passes above.
                s.into()
            }),
            Some(Ok(Token::RawString(.., s))) => Ok(s.into()),
            Some(Ok(Token::Null(..))) => Ok(FilterValue::Null),
            Some(Ok(token)) => Err(human_errors::user(
                format!("While parsing your filter, we found an unexpected '{}' at {}.", token, token.location()),
                &["Make sure that you have written a valid filter query."],
            )),
            Some(Err(err)) => Err(err),
            None => Err(human_errors::user(
                "We reached the end of your filter expression while waiting for a [true, false, \"string\", number, (group), or property.name].",
                &["Make sure that you have written a valid filter query and that you haven't forgotten part of it."],
            )),
          }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::{FilterValue, location::Loc};

    use super::*;

    #[rstest]
    #[case("true", true.into())]
    #[case("false", false.into())]
    #[case("\"hello\"", "hello".into())]
    #[case("\"escaped \\\"quotes\\\"\"", "escaped \"quotes\"".into())]
    #[case("123", 123.0.into())]
    #[case("null", FilterValue::Null)]
    #[case("[]", FilterValue::Tuple(vec![]))]
    #[case("[true]", FilterValue::Tuple(vec![true.into()]))]
    #[case("[\ntrue,\n]", FilterValue::Tuple(vec![true.into()]))]
    #[case("[true, false, \"test\", 123, null]", FilterValue::Tuple(vec![true.into(), false.into(), "test".into(), 123.into(), FilterValue::Null]))]
    fn parsing_literals(#[case] input: &str, #[case] value: FilterValue) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter()) {
            Ok(Expr::Literal(ast)) => assert_eq!(value, ast, "Expected {ast} to be {value}"),
            Ok(expr) => panic!("Expected a literal, got {:?}", expr),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[rstest]
    #[case("!true", Expr::Unary(Token::Not(Loc::new(1, 1)), Box::new(Expr::Literal(true.into()))))]
    #[case("!false", Expr::Unary(Token::Not(Loc::new(1, 1)), Box::new(Expr::Literal(false.into()))))]
    #[case("!\"hello\"", Expr::Unary(Token::Not(Loc::new(1, 1)), Box::new(Expr::Literal("hello".into()))))]
    #[case("!!true", Expr::Unary(Token::Not(Loc::new(1, 1)), Box::new(Expr::Unary(Token::Not(Loc::new(1, 2)), Box::new(Expr::Literal(true.into()))))))]
    fn parsing_unary_expressions(#[case] input: &str, #[case] ast: Expr) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter()) {
            Ok(expr) => assert_eq!(ast, expr, "Expected {ast} to be {expr}"),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[rstest]
    #[case("true == false", Expr::Binary(Box::new(Expr::Literal(true.into())), Token::Equals(Loc::new(1, 6)), Box::new(Expr::Literal(false.into()))))]
    #[case("true != false", Expr::Binary(Box::new(Expr::Literal(true.into())), Token::NotEquals(Loc::new(1, 6)), Box::new(Expr::Literal(false.into()))))]
    #[case("\"xyz\" startswith \"x\"", Expr::Binary(Box::new(Expr::Literal("xyz".into())), Token::StartsWith(Loc::new(1, 7)), Box::new(Expr::Literal("x".into()))))]
    #[case("\"xyz\" endswith \"z\"", Expr::Binary(Box::new(Expr::Literal("xyz".into())), Token::EndsWith(Loc::new(1, 7)), Box::new(Expr::Literal("z".into()))))]
    #[case("1 < 2", Expr::Binary(Box::new(Expr::Literal(1.0.into())), Token::SmallerThan(Loc::new(1, 2)), Box::new(Expr::Literal(2.0.into()))))]
    #[case("1 > 2", Expr::Binary(Box::new(Expr::Literal(1.0.into())), Token::GreaterThan(Loc::new(1, 2)), Box::new(Expr::Literal(2.0.into()))))]
    #[case("1 <= 2", Expr::Binary(Box::new(Expr::Literal(1.0.into())), Token::SmallerEqual(Loc::new(1, 3)), Box::new(Expr::Literal(2.0.into()))))]
    #[case("1 >= 2", Expr::Binary(Box::new(Expr::Literal(1.0.into())), Token::GreaterEqual(Loc::new(1, 3)), Box::new(Expr::Literal(2.0.into()))))]
    #[case("\"xyz\" contains_cs \"x\"", Expr::Binary(Box::new(Expr::Literal("xyz".into())), Token::ContainsCs(Loc::new(1, 7)), Box::new(Expr::Literal("x".into()))))]
    #[case("\"x\" in_cs \"xyz\"", Expr::Binary(Box::new(Expr::Literal("x".into())), Token::InCs(Loc::new(1, 5)), Box::new(Expr::Literal("xyz".into()))))]
    #[case("\"xyz\" startswith_cs \"x\"", Expr::Binary(Box::new(Expr::Literal("xyz".into())), Token::StartsWithCs(Loc::new(1, 7)), Box::new(Expr::Literal("x".into()))))]
    #[case("\"xyz\" endswith_cs \"z\"", Expr::Binary(Box::new(Expr::Literal("xyz".into())), Token::EndsWithCs(Loc::new(1, 7)), Box::new(Expr::Literal("z".into()))))]
    fn parse_comparison_expressions(#[case] input: &str, #[case] ast: Expr) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter()) {
            Ok(expr) => assert_eq!(ast, expr, "Expected {ast} to be {expr}"),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[rstest]
    #[case(
        "name like \"feat/*\"",
        Expr::Like(Box::new(Expr::Property("name")), Glob::compile("feat/*"))
    )]
    #[case(
        "name like r\"feat/\\*\"",
        Expr::Like(Box::new(Expr::Property("name")), Glob::compile("feat/\\*"))
    )]
    #[case(
        "name like \"say \\\"hi\\\"\"",
        Expr::Like(Box::new(Expr::Property("name")), Glob::compile("say \"hi\""))
    )]
    #[case("\"feat/login\" like \"feat/*\"", Expr::Like(Box::new(Expr::Literal("feat/login".into())), Glob::compile("feat/*")))]
    #[case(
        "name like_cs \"Feat/*\"",
        Expr::Like(Box::new(Expr::Property("name")), Glob::compile_cs("Feat/*"))
    )]
    #[case(
        "name like_cs r\"Feat/\\*\"",
        Expr::Like(Box::new(Expr::Property("name")), Glob::compile_cs("Feat/\\*"))
    )]
    fn parsing_like_expressions(#[case] input: &str, #[case] ast: Expr) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter()) {
            Ok(expr) => assert_eq!(ast, expr, "Expected {ast} to be {expr}"),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[cfg(feature = "regex")]
    #[rstest]
    #[case(
        "name matches r\"^release/v\\d+(\\.\\d+){2}$\"",
        "^release/v\\d+(\\.\\d+){2}$"
    )]
    #[case("name matches \"^release/v\\\\d+$\"", "^release/v\\d+$")]
    fn parsing_matches_expressions(#[case] input: &str, #[case] pattern: &str) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter()) {
            Ok(Expr::Matches(left, regex)) => {
                assert_eq!(*left, Expr::Property("name"));
                assert_eq!(regex.pattern(), pattern);
            }
            Ok(expr) => panic!("Expected a matches expression, got {:?}", expr),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[cfg(feature = "regex")]
    #[test]
    fn parsing_invalid_regex_patterns_fails_with_details() {
        let tokens = crate::lexer::Scanner::new("name matches r\"(unclosed\"");
        match Parser::parse(tokens.into_iter()) {
            Ok(expr) => panic!("Expected an error, got {:?}", expr),
            Err(e) => {
                let message = e.to_string();
                assert!(
                    message.contains(
                        "Failed to compile the regular expression pattern \"(unclosed\" for the 'matches' operator at line 1, column 6."
                    ),
                    "unexpected error: {message}"
                );
                // The underlying regex crate's error details should be included.
                assert!(
                    message.contains("unclosed group"),
                    "unexpected error: {message}"
                );
            }
        }
    }

    #[cfg(not(feature = "regex"))]
    #[test]
    fn parsing_matches_without_the_regex_feature_fails_with_advice() {
        let tokens = crate::lexer::Scanner::new("name matches r\"^v\\d+$\"");
        match Parser::parse(tokens.into_iter()) {
            Ok(expr) => panic!("Expected an error, got {:?}", expr),
            Err(e) => {
                let message = e.to_string();
                assert!(
                    message.contains(
                        "Your filter uses the 'matches' operator at line 1, column 6, but this build of the filters crate does not include regular expression support."
                    ),
                    "unexpected error: {message}"
                );
                assert!(
                    message.contains("Enable the 'regex' feature"),
                    "unexpected error: {message}"
                );
            }
        }
    }

    #[rstest]
    #[case(
        "name like other.name",
        "The 'like' operator at line 1, column 6 must be followed by its pattern as a string literal, but we found 'other.name' at line 1, column 11 instead"
    )]
    #[case(
        "name like_cs other.name",
        "The 'like_cs' operator at line 1, column 6 must be followed by its pattern as a string literal, but we found 'other.name' at line 1, column 14 instead"
    )]
    #[case(
        "name like 5",
        "The 'like' operator at line 1, column 6 must be followed by its pattern as a string literal, but we found '5' at line 1, column 11 instead"
    )]
    #[case(
        "name like true",
        "The 'like' operator at line 1, column 6 must be followed by its pattern as a string literal, but we found 'true' at line 1, column 11 instead"
    )]
    #[case(
        "name like",
        "We reached the end of your filter expression while looking for the string pattern of the 'like' operator at line 1, column 6"
    )]
    #[case(
        "name like \"unterminated",
        "Reached the end of the filter without finding the closing quote for a string starting at line 1, column 11"
    )]
    #[case(
        "name like r\"unterminated",
        "Reached the end of the filter without finding the closing quote for a raw string starting at line 1, column 11"
    )]
    fn invalid_like_patterns(#[case] input: &str, #[case] message: &str) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter()) {
            Ok(expr) => panic!("Expected an error, got {:?}", expr),
            Err(e) => assert!(
                e.to_string().contains(message),
                "Expected error message to contain '{}', got '{}'",
                message,
                e
            ),
        }
    }

    #[cfg(feature = "regex")]
    #[rstest]
    #[case(
        "name matches other.name",
        "The 'matches' operator at line 1, column 6 must be followed by its pattern as a string literal, but we found 'other.name' at line 1, column 14 instead"
    )]
    #[case(
        "name matches",
        "We reached the end of your filter expression while looking for the string pattern of the 'matches' operator at line 1, column 6"
    )]
    fn invalid_matches_patterns(#[case] input: &str, #[case] message: &str) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter()) {
            Ok(expr) => panic!("Expected an error, got {:?}", expr),
            Err(e) => assert!(
                e.to_string().contains(message),
                "Expected error message to contain '{}', got '{}'",
                message,
                e
            ),
        }
    }

    #[test]
    fn raw_strings_are_plain_string_literals() {
        let tokens = crate::lexer::Scanner::new("r\"a\\d\"");
        match Parser::parse(tokens.into_iter()) {
            Ok(Expr::Literal(value)) => assert_eq!(value, "a\\d".into()),
            Ok(expr) => panic!("Expected a literal, got {:?}", expr),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[rstest]
    #[case("true && false", Expr::Logical(Box::new(Expr::Literal(true.into())), Token::And(Loc::new(1, 6)), Box::new(Expr::Literal(false.into()))))]
    #[case("true || false", Expr::Logical(Box::new(Expr::Literal(true.into())), Token::Or(Loc::new(1, 6)), Box::new(Expr::Literal(false.into()))))]
    #[case("true && (true || false)", Expr::Logical(Box::new(Expr::Literal(true.into())), Token::And(Loc::new(1, 6)), Box::new(Expr::Logical(Box::new(Expr::Literal(true.into())), Token::Or(Loc::new(1, 15)), Box::new(Expr::Literal(false.into()))))))]
    fn parsing_logical_expressions(#[case] input: &str, #[case] ast: Expr) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter()) {
            Ok(expr) => assert_eq!(ast, expr, "Expected {ast} to be {expr}"),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[rstest]
    #[case(
        "true false",
        "Your filter expression contained an unexpected 'false' at line 1, column 6."
    )]
    #[case(
        "true ==",
        "We reached the end of your filter expression while waiting for a [true, false, \"string\", number, (group), or property.name]."
    )]
    #[case(
        "(true",
        "When attempting to parse a grouped filter expression starting at line 1, column 1, we didn't find the closing ')' where we expected to."
    )]
    #[case(
        "[true, false",
        "When attempting to parse a list filter expression starting at line 1, column 1, we didn't find the closing ']' where we expected to."
    )]
    #[case(
        ")",
        "While parsing your filter, we found an unexpected ')' at line 1, column 1."
    )]
    #[case(
        "[true && false]",
        "When attempting to parse a list filter expression starting at line 1, column 1, we didn't find the closing ']' where we expected to."
    )]
    #[case(
        "a == &",
        "Filter included an orphaned '&' at line 1, column 6 which is not a valid operator."
    )]
    #[case(
        "nope()",
        "Your filter called an unknown function 'nope()' at line 1, column 1."
    )]
    #[case(
        "nope(1, true)",
        "Your filter called an unknown function 'nope()' at line 1, column 1."
    )]
    #[case(
        "now(1",
        "When attempting to parse the arguments to the 'now()' function call at line 1, column 1, we didn't find the closing ')' where we expected to."
    )]
    fn invalid_filters(#[case] input: &str, #[case] message: &str) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter()) {
            Ok(expr) => panic!("Expected an error, got {:?}", expr),
            Err(e) => assert!(
                e.to_string().contains(message),
                "Expected error message to contain '{}', got '{}'",
                message,
                e
            ),
        }
    }

    #[test]
    fn unknown_function_errors_list_the_supported_functions() {
        let tokens = crate::lexer::Scanner::new("nope()");
        let error = Parser::parse(tokens.into_iter()).expect_err("the filter should fail to parse");
        assert!(
            error.to_string().contains("now()"),
            "Expected the error to list the supported functions, got '{error}'"
        );
    }

    #[test]
    fn now_requires_the_chrono_feature() {
        let tokens = crate::lexer::Scanner::new("now()");
        let error = Parser::parse(tokens.into_iter()).expect_err("the filter should fail to parse");
        assert!(
            error.to_string().contains("'chrono' feature"),
            "Expected the error to mention the 'chrono' feature, got '{error}'"
        );
    }
}
