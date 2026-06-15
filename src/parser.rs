use std::iter::Peekable;
use std::sync::Arc;

use human_errors::{Error, ResultExt};

use super::{
    FilterValue, expr::Expr, functions::Function, location::Loc, pattern::Glob, token::Token,
};

pub struct Parser<'a, 'f, I: Iterator<Item = Result<Token<'a>, Error>>> {
    tokens: Peekable<I>,
    /// The functions which may be called from the expression. Function calls are
    /// resolved against (and validated by) this set at parse time; the matched
    /// [`Function`] is cloned into the resulting [`Expr::FunctionCall`] node so
    /// evaluation can dispatch straight to it.
    functions: &'f [Arc<dyn Function>],
}

impl<'a, 'f, I: Iterator<Item = Result<Token<'a>, Error>>> Parser<'a, 'f, I> {
    pub fn parse(tokens: I, functions: &'f [Arc<dyn Function>]) -> Result<Expr<'a>, Error> {
        let mut parser = Parser {
            tokens: tokens.peekable(),
            functions,
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
            let operator = self.tokens.next().unwrap()?.as_logical_operator();
            let right = self.and()?;
            expr = Expr::Logical(Box::new(expr), operator, Box::new(right));
        }

        Ok(expr)
    }

    fn and(&mut self) -> Result<Expr<'a>, Error> {
        let mut expr = self.equality()?;

        while matches!(self.tokens.peek(), Some(Ok(Token::And(..)))) {
            let operator = self.tokens.next().unwrap()?.as_logical_operator();
            let right = self.equality()?;
            expr = Expr::Logical(Box::new(expr), operator, Box::new(right));
        }

        Ok(expr)
    }

    fn equality(&mut self) -> Result<Expr<'a>, Error> {
        let mut expr = self.comparison()?;

        if matches!(
            self.tokens.peek(),
            Some(Ok(Token::Equals(..)) | Ok(Token::NotEquals(..)))
        ) {
            let operator = self.tokens.next().unwrap()?.as_binary_operator();
            let right = self.comparison()?;
            expr = Expr::Binary(Box::new(expr), operator, Box::new(right));
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
            let operator = self.tokens.next().unwrap()?.as_binary_operator();
            let right = self.term()?;
            expr = Expr::Binary(Box::new(expr), operator, Box::new(right));
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
                    "Your filter uses the 'matches' operator at {}, but this build of the filt-rs crate does not include regular expression support.",
                    token.location()
                ),
                &[
                    "Enable the 'regex' feature of the filt-rs crate to use the 'matches' operator (e.g. filt-rs = { version = \"0.1\", features = [\"regex\"] }).",
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
            let operator = self.tokens.next().unwrap()?.as_binary_operator();
            let right = self.unary()?;
            expr = Expr::Binary(Box::new(expr), operator, Box::new(right));
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
            let operator = self.tokens.next().unwrap()?.as_unary_operator();
            let right = self.unary()?;
            Ok(Expr::Unary(operator, Box::new(right)))
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

        // Resolve the call against the registered functions, validating its
        // arity at parse time so a bad call fails fast with a friendly error.
        // The matched function is cloned (a cheap reference-count bump) into the
        // AST so evaluation can dispatch straight to it.
        let functions = self.functions;
        match functions.iter().find(|function| function.name() == name) {
            Some(function) if function.arity() == args.len() => {
                Ok(Expr::FunctionCall(Arc::clone(function), args))
            }
            Some(function) => Err(arity_error(name, loc, function.arity(), args.len())),
            None => Err(unknown_function(name, loc, functions)),
        }
    }

    /// Converts a duration lexeme such as `5m`, `500ms`, or `1h30m` into a
    /// [`FilterValue::Duration`]. The lexer has already validated the shape of
    /// the lexeme, so this only needs to accumulate the segments.
    #[cfg(feature = "chrono")]
    fn duration_literal(loc: Loc, lexeme: &str) -> Result<FilterValue<'a>, Error> {
        let mut total_ms = 0.0_f64;
        let mut rest = lexeme;

        while !rest.is_empty() {
            let number_len = rest.find(|c: char| c.is_alphabetic()).unwrap_or(rest.len());
            let (number, tail) = rest.split_at(number_len);
            let unit_len = tail
                .find(|c: char| !c.is_alphabetic())
                .unwrap_or(tail.len());
            let (unit, tail) = tail.split_at(unit_len);
            rest = tail;

            let value: f64 = number.parse().wrap_user_err(
                format!(
                    "Failed to parse the duration '{lexeme}' which you provided at {loc}."
                ),
                &["Please make sure that the duration is well formatted. It should be in the form 90s, 1h30m, or 500ms."],
            )?;

            let scale_ms = match unit {
                "ms" => 1.0,
                "s" => 1_000.0,
                "m" => 60_000.0,
                "h" => 3_600_000.0,
                "d" => 86_400_000.0,
                "w" => 604_800_000.0,
                _ => {
                    return Err(human_errors::user(
                        format!(
                            "The duration '{lexeme}' at {loc} used the unit '{unit}', which is not a recognized duration unit."
                        ),
                        &[
                            "Use one of the supported duration units: 'ms' (milliseconds), 's' (seconds), 'm' (minutes), 'h' (hours), 'd' (days), or 'w' (weeks).",
                        ],
                    ));
                }
            };

            total_ms += value * scale_ms;
        }

        Ok(FilterValue::Duration(chrono::Duration::milliseconds(
            total_ms.round() as i64,
        )))
    }

    /// Without the `chrono` feature there is no duration type to parse into,
    /// so duration literals are rejected with advice to enable the feature.
    #[cfg(not(feature = "chrono"))]
    fn duration_literal(loc: Loc, lexeme: &str) -> Result<FilterValue<'a>, Error> {
        Err(human_errors::user(
            format!(
                "Your filter used the duration '{lexeme}' at {loc}, but datetime support is not enabled in this build."
            ),
            &[
                "Enable the 'chrono' feature of the filt-rs crate to use duration literals like '5m' or '1h30m'.",
            ],
        ))
    }

    fn literal(&mut self) -> Result<FilterValue<'a>, Error> {
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
            Some(Ok(Token::Duration(loc, d))) => Self::duration_literal(loc, d),
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

/// Builds the parse-time error for a function call with the wrong number of
/// arguments.
fn arity_error(name: &str, loc: Loc, expected: usize, actual: usize) -> Error {
    let plural = if expected == 1 { "" } else { "s" };
    // The advice must be `'static`, so all the specifics go in the message.
    human_errors::user(
        format!(
            "The '{name}()' function at {loc} expects {expected} argument{plural}, but your filter provided {actual}."
        ),
        &["Adjust the call so that it passes the number of arguments the function expects."],
    )
}

/// Builds the parse-time error for a call to a function which is not registered,
/// listing the functions which *are* available so the writer can correct it.
fn unknown_function(name: &str, loc: Loc, functions: &[Arc<dyn Function>]) -> Error {
    // Without the `chrono` feature, `now()` is not registered. Keep the tailored
    // hint the crate documents rather than treating it as a plain typo.
    #[cfg(not(feature = "chrono"))]
    if name == "now" {
        return human_errors::user(
            format!(
                "Your filter called the 'now()' function at {loc}, but datetime support is not enabled in this build."
            ),
            &[
                "Enable the 'chrono' feature of the filt-rs crate to use datetime functions like 'now()'.",
            ],
        );
    }

    // The available list is dynamic, so it goes in the message (the advice has
    // to be `'static`).
    let available = functions
        .iter()
        .map(|function| format!("{}()", function.name()))
        .collect::<Vec<_>>()
        .join(", ");
    human_errors::user(
        format!(
            "Your filter called an unknown function '{name}()' at {loc}. The functions available in this filter are: {available}."
        ),
        &[
            "Make sure you are calling one of the supported functions, or register your own with Filter::with_functions.",
        ],
    )
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::{
        FilterValue,
        functions::base_functions,
        operator::{BinaryOperator, LogicalOperator, UnaryOperator},
    };

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
    fn parsing_literals(#[case] input: &str, #[case] value: FilterValue<'_>) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter(), &base_functions()) {
            Ok(Expr::Literal(ast)) => assert_eq!(value, ast, "Expected {ast} to be {value}"),
            Ok(expr) => panic!("Expected a literal, got {:?}", expr),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[rstest]
    #[case("!true", Expr::Unary(UnaryOperator::Not, Box::new(Expr::Literal(true.into()))))]
    #[case("!false", Expr::Unary(UnaryOperator::Not, Box::new(Expr::Literal(false.into()))))]
    #[case("!\"hello\"", Expr::Unary(UnaryOperator::Not, Box::new(Expr::Literal("hello".into()))))]
    #[case("!!true", Expr::Unary(UnaryOperator::Not, Box::new(Expr::Unary(UnaryOperator::Not, Box::new(Expr::Literal(true.into()))))))]
    fn parsing_unary_expressions(#[case] input: &str, #[case] ast: Expr<'_>) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter(), &base_functions()) {
            Ok(expr) => assert_eq!(ast, expr, "Expected {ast} to be {expr}"),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[rstest]
    #[case("true == false", Expr::Binary(Box::new(Expr::Literal(true.into())), BinaryOperator::Equals, Box::new(Expr::Literal(false.into()))))]
    #[case("true != false", Expr::Binary(Box::new(Expr::Literal(true.into())), BinaryOperator::NotEquals, Box::new(Expr::Literal(false.into()))))]
    #[case("\"xyz\" startswith \"x\"", Expr::Binary(Box::new(Expr::Literal("xyz".into())), BinaryOperator::StartsWith, Box::new(Expr::Literal("x".into()))))]
    #[case("\"xyz\" endswith \"z\"", Expr::Binary(Box::new(Expr::Literal("xyz".into())), BinaryOperator::EndsWith, Box::new(Expr::Literal("z".into()))))]
    #[case("1 < 2", Expr::Binary(Box::new(Expr::Literal(1.0.into())), BinaryOperator::SmallerThan, Box::new(Expr::Literal(2.0.into()))))]
    #[case("1 > 2", Expr::Binary(Box::new(Expr::Literal(1.0.into())), BinaryOperator::GreaterThan, Box::new(Expr::Literal(2.0.into()))))]
    #[case("1 <= 2", Expr::Binary(Box::new(Expr::Literal(1.0.into())), BinaryOperator::SmallerEqual, Box::new(Expr::Literal(2.0.into()))))]
    #[case("1 >= 2", Expr::Binary(Box::new(Expr::Literal(1.0.into())), BinaryOperator::GreaterEqual, Box::new(Expr::Literal(2.0.into()))))]
    #[case("\"xyz\" contains_cs \"x\"", Expr::Binary(Box::new(Expr::Literal("xyz".into())), BinaryOperator::ContainsCs, Box::new(Expr::Literal("x".into()))))]
    #[case("\"x\" in_cs \"xyz\"", Expr::Binary(Box::new(Expr::Literal("x".into())), BinaryOperator::InCs, Box::new(Expr::Literal("xyz".into()))))]
    #[case("\"xyz\" startswith_cs \"x\"", Expr::Binary(Box::new(Expr::Literal("xyz".into())), BinaryOperator::StartsWithCs, Box::new(Expr::Literal("x".into()))))]
    #[case("\"xyz\" endswith_cs \"z\"", Expr::Binary(Box::new(Expr::Literal("xyz".into())), BinaryOperator::EndsWithCs, Box::new(Expr::Literal("z".into()))))]
    fn parse_comparison_expressions(#[case] input: &str, #[case] ast: Expr<'_>) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter(), &base_functions()) {
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
    fn parsing_like_expressions(#[case] input: &str, #[case] ast: Expr<'_>) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter(), &base_functions()) {
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
        match Parser::parse(tokens.into_iter(), &base_functions()) {
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
        match Parser::parse(tokens.into_iter(), &base_functions()) {
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
        match Parser::parse(tokens.into_iter(), &base_functions()) {
            Ok(expr) => panic!("Expected an error, got {:?}", expr),
            Err(e) => {
                let message = e.to_string();
                assert!(
                    message.contains(
                        "Your filter uses the 'matches' operator at line 1, column 6, but this build of the filt-rs crate does not include regular expression support."
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
        match Parser::parse(tokens.into_iter(), &base_functions()) {
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
        match Parser::parse(tokens.into_iter(), &base_functions()) {
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
        match Parser::parse(tokens.into_iter(), &base_functions()) {
            Ok(Expr::Literal(value)) => assert_eq!(value, "a\\d".into()),
            Ok(expr) => panic!("Expected a literal, got {:?}", expr),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn hashed_raw_strings_are_plain_string_literals() {
        // The hashed form carries its embedded quotes through verbatim, which is
        // what makes it convenient for representing JSON.
        let tokens = crate::lexer::Scanner::new("r#\"{\"status\":\"ok\"}\"#");
        match Parser::parse(tokens.into_iter(), &base_functions()) {
            Ok(Expr::Literal(value)) => assert_eq!(value, "{\"status\":\"ok\"}".into()),
            Ok(expr) => panic!("Expected a literal, got {:?}", expr),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[rstest]
    #[case("true && false", Expr::Logical(Box::new(Expr::Literal(true.into())), LogicalOperator::And, Box::new(Expr::Literal(false.into()))))]
    #[case("true || false", Expr::Logical(Box::new(Expr::Literal(true.into())), LogicalOperator::Or, Box::new(Expr::Literal(false.into()))))]
    #[case("true && (true || false)", Expr::Logical(Box::new(Expr::Literal(true.into())), LogicalOperator::And, Box::new(Expr::Logical(Box::new(Expr::Literal(true.into())), LogicalOperator::Or, Box::new(Expr::Literal(false.into()))))))]
    fn parsing_logical_expressions(#[case] input: &str, #[case] ast: Expr<'_>) {
        let tokens = crate::lexer::Scanner::new(input);
        match Parser::parse(tokens.into_iter(), &base_functions()) {
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
        match Parser::parse(tokens.into_iter(), &base_functions()) {
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
        let error = Parser::parse(tokens.into_iter(), &base_functions())
            .expect_err("the filter should fail to parse");
        // The available list is built from the registered functions, so `now()`
        // only appears when the chrono feature includes it.
        #[cfg(feature = "chrono")]
        assert!(
            error.to_string().contains("now()"),
            "Expected the error to list the supported functions, got '{error}'"
        );
        assert!(
            error.to_string().contains("trim()"),
            "Expected the error to list the supported functions, got '{error}'"
        );
    }

    #[test]
    fn trim_parses_to_a_function_call() {
        let tokens = crate::lexer::Scanner::new("trim(name)");
        match Parser::parse(tokens.into_iter(), &base_functions()) {
            Ok(Expr::FunctionCall(function, args)) => {
                assert_eq!(function.name(), "trim");
                assert_eq!(args.len(), 1);
                assert_eq!(args[0], Expr::Property("name"));
            }
            Ok(expr) => panic!("Expected a function call, got {:?}", expr),
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[rstest]
    #[case(
        "trim()",
        "The 'trim()' function at line 1, column 1 expects 1 argument, but your filter provided 0."
    )]
    #[case(
        "trim(name, other)",
        "The 'trim()' function at line 1, column 1 expects 1 argument, but your filter provided 2."
    )]
    fn trim_requires_exactly_one_argument(#[case] input: &str, #[case] message: &str) {
        let tokens = crate::lexer::Scanner::new(input);
        let error = Parser::parse(tokens.into_iter(), &base_functions())
            .expect_err("the filter should fail to parse");
        assert!(
            error.to_string().contains(message),
            "Expected error message to contain '{}', got '{}'",
            message,
            error
        );
    }

    #[cfg(not(feature = "chrono"))]
    #[test]
    fn now_requires_the_chrono_feature() {
        let tokens = crate::lexer::Scanner::new("now()");
        let error = Parser::parse(tokens.into_iter(), &base_functions())
            .expect_err("the filter should fail to parse");
        assert!(
            error.to_string().contains("'chrono' feature"),
            "Expected the error to mention the 'chrono' feature, got '{error}'"
        );
    }

    #[cfg(not(feature = "chrono"))]
    #[test]
    fn durations_require_the_chrono_feature() {
        let tokens = crate::lexer::Scanner::new("5m");
        let error = Parser::parse(tokens.into_iter(), &base_functions())
            .expect_err("the filter should fail to parse");
        assert!(
            error.to_string().contains("'chrono' feature"),
            "Expected the error to mention the 'chrono' feature, got '{error}'"
        );
    }

    #[cfg(feature = "chrono")]
    mod chrono_tests {
        use super::*;

        #[test]
        fn now_parses_to_a_function_call() {
            let tokens = crate::lexer::Scanner::new("now()");
            match Parser::parse(tokens.into_iter(), &base_functions()) {
                Ok(Expr::FunctionCall(function, args)) => {
                    assert_eq!(function.name(), "now");
                    assert!(args.is_empty());
                }
                Ok(expr) => panic!("Expected a function call, got {:?}", expr),
                Err(e) => panic!("Error: {}", e),
            }
        }

        #[test]
        fn now_rejects_arguments_at_parse_time() {
            let tokens = crate::lexer::Scanner::new("now(1)");
            let error = Parser::parse(tokens.into_iter(), &base_functions())
                .expect_err("the filter should fail to parse");
            assert!(
                error.to_string().contains(
                    "The 'now()' function at line 1, column 1 expects 0 arguments, but your filter provided 1."
                ),
                "Expected an arity error, got '{error}'"
            );
        }

        #[rstest]
        #[case("500ms", chrono::Duration::milliseconds(500))]
        #[case("90s", chrono::Duration::seconds(90))]
        #[case("5m", chrono::Duration::minutes(5))]
        #[case("2h", chrono::Duration::hours(2))]
        #[case("7d", chrono::Duration::days(7))]
        #[case("1w", chrono::Duration::weeks(1))]
        #[case("1h30m", chrono::Duration::minutes(90))]
        #[case("1w2d3h4m5s6ms", chrono::Duration::milliseconds(788_645_006))]
        #[case("1.5h", chrono::Duration::minutes(90))]
        #[case("0s", chrono::Duration::zero())]
        fn duration_literals(#[case] input: &str, #[case] expected: chrono::Duration) {
            let tokens = crate::lexer::Scanner::new(input);
            match Parser::parse(tokens.into_iter(), &base_functions()) {
                Ok(Expr::Literal(FilterValue::Duration(d))) => assert_eq!(d, expected),
                Ok(expr) => panic!("Expected a duration literal, got {:?}", expr),
                Err(e) => panic!("Error: {}", e),
            }
        }
    }
}
