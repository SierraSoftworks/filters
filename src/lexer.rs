use super::{location::Loc, token::Token};

pub struct Scanner<'a> {
    source: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    line: usize,
    line_start: usize,
}

impl<'a> Scanner<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            line: 1,
            line_start: 0,
        }
    }

    fn match_char(&mut self, next: char) -> bool {
        if let Some((idx, c)) = self.chars.peek() {
            if *c == '\n' {
                self.line += 1;
                self.line_start = *idx + 1;
            }

            if *c == next {
                self.chars.next();
                return true;
            }
        }

        false
    }

    fn advance_while_fn<F: Fn(usize, char) -> bool>(&mut self, f: F) -> usize {
        let mut length = 0;
        while let Some((idx, c)) = self.chars.peek() {
            if *c == '\n' {
                self.line += 1;
                self.line_start = *idx + 1;
            }

            if !f(*idx, *c) {
                break;
            }

            self.chars.next();
            length += 1;
        }

        length
    }

    fn read_string(&mut self, start: usize) -> Result<Token<'a>, human_errors::Error> {
        let start_loc = Loc::new(self.line, 1 + start - self.line_start);
        while let Some((idx, c)) = self.chars.next() {
            match c {
                '\n' => {
                    self.line += 1;
                    self.line_start = idx + 1;
                }
                '"' => {
                    return Ok(Token::String(start_loc, &self.source[start + 1..idx]));
                }
                '\\' if self.match_char('"') => {}
                _ => {}
            }
        }

        Err(human_errors::user(
            format!(
                "Reached the end of the filter without finding the closing quote for a string starting at {}.",
                start_loc
            ),
            &["Make sure that you have terminated your string with a '\"' character."],
        ))
    }

    fn read_raw_string(&mut self, start: usize) -> Result<Token<'a>, human_errors::Error> {
        // `start` is the index of the `r` prefix; the opening quote (one byte
        // further along) has already been consumed by the caller.
        let start_loc = Loc::new(self.line, 1 + start - self.line_start);
        for (idx, c) in self.chars.by_ref() {
            match c {
                '\n' => {
                    self.line += 1;
                    self.line_start = idx + 1;
                }
                '"' => {
                    return Ok(Token::RawString(start_loc, &self.source[start + 2..idx]));
                }
                _ => {}
            }
        }

        Err(human_errors::user(
            format!(
                "Reached the end of the filter without finding the closing quote for a raw string starting at {}.",
                start_loc
            ),
            &[
                "Make sure that you have terminated your raw string with a '\"' character.",
                "Raw strings do not support escape sequences, so they cannot contain '\"' characters.",
            ],
        ))
    }

    fn read_number(&mut self, start: usize) -> Result<Token<'a>, human_errors::Error> {
        let mut end = start + self.advance_while_fn(|_, c| c.is_numeric());
        if let Some((loc, c)) = self.chars.peek()
            && *c == '.'
            && self.source[loc + 1..]
                .chars()
                .next()
                .map(|c2| c2.is_numeric())
                .unwrap_or_default()
        {
            self.chars.next();
            end += 1 + self.advance_while_fn(|_, c| c.is_numeric());
        }

        Ok(Token::Number(
            Loc::new(self.line, 1 + start - self.line_start),
            &self.source[start..end + 1],
        ))
    }

    fn read_identifier(&mut self, start: usize) -> Result<Token<'a>, human_errors::Error> {
        let end = start
            + self.advance_while_fn(|_, c| c.is_alphanumeric() || c == '_' || c == '.' || c == '-');
        let lexeme = &self.source[start..end + 1];
        let location = Loc::new(self.line, 1 + start - self.line_start);

        match lexeme {
            "false" => Ok(Token::False(location)),
            "null" => Ok(Token::Null(location)),
            "true" => Ok(Token::True(location)),
            "contains" => Ok(Token::Contains(location)),
            "contains_cs" => Ok(Token::ContainsCs(location)),
            "in" => Ok(Token::In(location)),
            "in_cs" => Ok(Token::InCs(location)),
            "startswith" => Ok(Token::StartsWith(location)),
            "startswith_cs" => Ok(Token::StartsWithCs(location)),
            "endswith" => Ok(Token::EndsWith(location)),
            "endswith_cs" => Ok(Token::EndsWithCs(location)),
            "like" => Ok(Token::Like(location)),
            "like_cs" => Ok(Token::LikeCs(location)),
            "matches" => Ok(Token::Matches(location)),
            lexeme => Ok(Token::Property(location, lexeme)),
        }
    }
}

impl<'a> Iterator for Scanner<'a> {
    type Item = Result<Token<'a>, human_errors::Error>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((idx, c)) = self.chars.next() {
            match c {
                ' ' | '\t' => {}
                '\n' => {
                    self.line += 1;
                    self.line_start = idx + 1;
                }
                '(' => {
                    return Some(Ok(Token::LeftParen(Loc::new(
                        self.line,
                        1 + idx - self.line_start,
                    ))));
                }
                ')' => {
                    return Some(Ok(Token::RightParen(Loc::new(
                        self.line,
                        1 + idx - self.line_start,
                    ))));
                }
                '[' => {
                    return Some(Ok(Token::LeftBracket(Loc::new(
                        self.line,
                        1 + idx - self.line_start,
                    ))));
                }
                ']' => {
                    return Some(Ok(Token::RightBracket(Loc::new(
                        self.line,
                        1 + idx - self.line_start,
                    ))));
                }
                ',' => {
                    return Some(Ok(Token::Comma(Loc::new(
                        self.line,
                        1 + idx - self.line_start,
                    ))));
                }
                // NOTE: '+' and '-' only act as operators when they *start* a new
                // token; a '-' inside an identifier which is already being consumed
                // remains part of that identifier (e.g. `asset.source-code`).
                '+' => {
                    return Some(Ok(Token::Plus(Loc::new(
                        self.line,
                        1 + idx - self.line_start,
                    ))));
                }
                '-' => {
                    return Some(Ok(Token::Minus(Loc::new(
                        self.line,
                        1 + idx - self.line_start,
                    ))));
                }
                '&' => {
                    return if self.match_char('&') {
                        Some(Ok(Token::And(Loc::new(
                            self.line,
                            1 + idx - self.line_start,
                        ))))
                    } else {
                        Some(Err(human_errors::user(
                            format!(
                                "Filter included an orphaned '&' at {} which is not a valid operator.",
                                Loc::new(self.line, 1 + idx - self.line_start)
                            ),
                            &[
                                "Ensure that you are using the '&&' operator to implement a logical AND within your filter.",
                            ],
                        )))
                    };
                }
                '|' => {
                    return if self.match_char('|') {
                        Some(Ok(Token::Or(Loc::new(
                            self.line,
                            1 + idx - self.line_start,
                        ))))
                    } else {
                        Some(Err(human_errors::user(
                            format!(
                                "Filter included an orphaned '|' at {} which is not a valid operator.",
                                Loc::new(self.line, 1 + idx - self.line_start)
                            ),
                            &[
                                "Ensure that you are using the '||' operator to implement a logical OR within your filter.",
                            ],
                        )))
                    };
                }
                '=' => {
                    return if self.match_char('=') {
                        Some(Ok(Token::Equals(Loc::new(
                            self.line,
                            1 + idx - self.line_start,
                        ))))
                    } else {
                        Some(Err(human_errors::user(
                            format!(
                                "Filter included an orphaned '=' at {} which is not a valid operator.",
                                Loc::new(self.line, 1 + idx - self.line_start)
                            ),
                            &[
                                "Ensure that you are using the '==' operator to implement a logical equality within your filter.",
                            ],
                        )))
                    };
                }
                '!' => {
                    return if self.match_char('=') {
                        Some(Ok(Token::NotEquals(Loc::new(
                            self.line,
                            1 + idx - self.line_start,
                        ))))
                    } else {
                        Some(Ok(Token::Not(Loc::new(
                            self.line,
                            1 + idx - self.line_start,
                        ))))
                    };
                }
                '>' => {
                    return if self.match_char('=') {
                        Some(Ok(Token::GreaterEqual(Loc::new(
                            self.line,
                            1 + idx - self.line_start,
                        ))))
                    } else {
                        Some(Ok(Token::GreaterThan(Loc::new(
                            self.line,
                            idx - self.line_start,
                        ))))
                    };
                }
                '<' => {
                    return if self.match_char('=') {
                        Some(Ok(Token::SmallerEqual(Loc::new(
                            self.line,
                            1 + idx - self.line_start,
                        ))))
                    } else {
                        Some(Ok(Token::SmallerThan(Loc::new(
                            self.line,
                            idx - self.line_start,
                        ))))
                    };
                }
                '"' => {
                    return Some(self.read_string(idx));
                }
                'r' if matches!(self.chars.peek(), Some((_, '"'))) => {
                    self.chars.next();
                    return Some(self.read_raw_string(idx));
                }
                c if c.is_numeric() => {
                    return Some(self.read_number(idx));
                }
                _ => {
                    return Some(self.read_identifier(idx));
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    macro_rules! assert_sequence {
      ($filter:expr $(, $item:pat)* $(,)?) => {
        let mut scanner = Scanner::new($filter);
        $(
          match scanner.next() {
            Some(Ok($item)) => {},
            Some(Ok(item)) => panic!("Expected '{}' but got '{:?}'", stringify!($item), item),
            Some(Err(e)) => panic!("Error: {}", e),
            None => panic!("Expected '{}' but got the end of the parse sequence instead", stringify!($item)),
          }
        )*

        assert!(scanner.next().is_none(), "expected end of sequence, but got an item");
      };
    }

    #[test]
    fn test_empty() {
        assert_sequence!("");
    }

    #[test]
    fn test_whitespace() {
        assert_sequence!("  \t\n");
    }

    #[test]
    fn test_parens() {
        assert_sequence!(
            "() []",
            Token::LeftParen(..),
            Token::RightParen(..),
            Token::LeftBracket(..),
            Token::RightBracket(..),
        );
    }

    #[test]
    fn test_logical_operators() {
        assert_sequence!("&& ||", Token::And(..), Token::Or(..));
    }

    #[test]
    fn test_comparison_operators() {
        assert_sequence!(
            "== != contains in startswith endswith like matches > >= < <=",
            Token::Equals(..),
            Token::NotEquals(..),
            Token::Contains(..),
            Token::In(..),
            Token::StartsWith(..),
            Token::EndsWith(..),
            Token::Like(..),
            Token::Matches(..),
            Token::GreaterThan(..),
            Token::GreaterEqual(..),
            Token::SmallerThan(..),
            Token::SmallerEqual(..),
        );
    }

    #[test]
    fn test_case_sensitive_comparison_operators() {
        assert_sequence!(
            "contains_cs in_cs startswith_cs endswith_cs like_cs",
            Token::ContainsCs(..),
            Token::InCs(..),
            Token::StartsWithCs(..),
            Token::EndsWithCs(..),
            Token::LikeCs(..),
        );
    }

    #[test]
    fn test_cs_suffixed_identifiers_are_properties() {
        // Only the exact `_cs` keywords are reserved; anything longer is
        // still an ordinary property reference.
        assert_sequence!(
            "contains_csx in_cs2 like_cs.name",
            Token::Property(.., "contains_csx"),
            Token::Property(.., "in_cs2"),
            Token::Property(.., "like_cs.name"),
        );
    }

    #[test]
    fn test_raw_string() {
        assert_sequence!(
            "r\"^release/v\\d+(\\.\\d+){2}$\"",
            Token::RawString(.., "^release/v\\d+(\\.\\d+){2}$"),
        );

        // Escape sequences are not processed within raw strings, so backslashes
        // survive untouched (and a `\"` would terminate the string).
        assert_sequence!("r\"a\\\\b\"", Token::RawString(.., "a\\\\b"));

        // An `r` which isn't immediately followed by a quote is just an identifier.
        assert_sequence!(
            "release r2d2",
            Token::Property(.., "release"),
            Token::Property(.., "r2d2"),
        );
    }

    #[test]
    fn test_raw_string_location_tracking() {
        assert_sequence!(
            "r\"multi\nline\" && done",
            Token::RawString(Loc { line: 1, column: 1 }, "multi\nline"),
            Token::And(Loc { line: 2, column: 7 }),
            Token::Property(.., "done"),
        );
    }

    #[test]
    fn test_string() {
        assert_sequence!("\"hello world\"", Token::String(.., "hello world"));

        assert_sequence!(
            "\"hello \\\"world\\\"\"",
            Token::String(.., "hello \\\"world\\\""),
        );
    }

    #[test]
    fn test_number() {
        assert_sequence!("123.456", Token::Number(.., "123.456"));
    }

    #[rstest]
    #[case("0", "0")]
    #[case("123", "123")]
    #[case("123.456", "123.456")]
    #[case("0.5", "0.5")]
    fn test_number_formats(#[case] input: &str, #[case] lexeme: &str) {
        let mut scanner = Scanner::new(input);
        match scanner.next() {
            Some(Ok(Token::Number(_, l))) => assert_eq!(l, lexeme),
            other => panic!("Expected a number token, got {:?}", other),
        }
        assert!(scanner.next().is_none());
    }

    #[test]
    fn test_identifiers() {
        assert_sequence!(
            "true false null foo.bar-baz",
            Token::True(..),
            Token::False(..),
            Token::Null(..),
            Token::Property(.., "foo.bar-baz"),
        );
    }

    #[test]
    fn test_mixed() {
        assert_sequence!(
            "foo == \"bar\" && baz != 123",
            Token::Property(.., "foo"),
            Token::Equals(..),
            Token::String(.., "bar"),
            Token::And(..),
            Token::Property(.., "baz"),
            Token::NotEquals(..),
            Token::Number(.., "123"),
        );
    }

    #[test]
    fn test_negation() {
        assert_sequence!(
            "repo.public && !release.prerelease && !artifact.source-code",
            Token::Property(.., "repo.public"),
            Token::And(..),
            Token::Not(..),
            Token::Property(.., "release.prerelease"),
            Token::And(..),
            Token::Not(..),
            Token::Property(.., "artifact.source-code"),
        );
    }

    #[test]
    fn test_arithmetic_operators() {
        assert_sequence!(
            "1 + 2 - 3",
            Token::Number(.., "1"),
            Token::Plus(..),
            Token::Number(.., "2"),
            Token::Minus(..),
            Token::Number(.., "3"),
        );

        // No whitespace is required around the operators...
        assert_sequence!(
            "1+2-3",
            Token::Number(.., "1"),
            Token::Plus(..),
            Token::Number(.., "2"),
            Token::Minus(..),
            Token::Number(.., "3"),
        );

        // ...and a leading '-' is an operator rather than part of a number.
        assert_sequence!("-5", Token::Minus(..), Token::Number(.., "5"));
    }

    #[test]
    fn test_hyphenated_identifiers_are_not_subtraction() {
        // A '-' inside an identifier which is already being consumed remains
        // part of that identifier...
        assert_sequence!(
            "asset.source-code",
            Token::Property(.., "asset.source-code")
        );

        // ...while a '-' which starts a new token is the subtraction operator.
        assert_sequence!(
            "asset.size - 5",
            Token::Property(.., "asset.size"),
            Token::Minus(..),
            Token::Number(.., "5"),
        );

        // A '-' immediately following a property starts a new token only when
        // preceded by whitespace; without it, the identifier keeps growing.
        assert_sequence!("asset.size-5", Token::Property(.., "asset.size-5"));
    }

    #[test]
    fn test_location() {
        assert_sequence!(
            "true !=\nfalse",
            Token::True(Loc { line: 1, column: 1 }),
            Token::NotEquals(Loc { line: 1, column: 6 }),
            Token::False(Loc { line: 2, column: 1 })
        );
    }

    #[rstest]
    #[case("&", "Filter included an orphaned '&' at line 1, column 1")]
    #[case("|", "Filter included an orphaned '|' at line 1, column 1")]
    #[case("=", "Filter included an orphaned '=' at line 1, column 1")]
    #[case("a & b", "Filter included an orphaned '&' at line 1, column 3")]
    #[case(
        "\"unterminated",
        "Reached the end of the filter without finding the closing quote for a string starting at line 1, column 1"
    )]
    #[case(
        "r\"unterminated",
        "Reached the end of the filter without finding the closing quote for a raw string starting at line 1, column 1"
    )]
    fn test_lexing_errors(#[case] input: &str, #[case] message: &str) {
        let mut scanner = Scanner::new(input);
        let error = loop {
            match scanner.next() {
                Some(Ok(..)) => continue,
                Some(Err(e)) => break e,
                None => panic!("Expected an error while scanning '{input}'"),
            }
        };

        assert!(
            error.to_string().contains(message),
            "Expected error message to contain '{}', got '{}'",
            message,
            error
        );
    }

    #[test]
    fn test_multiline_strings() {
        assert_sequence!(
            "\"hello\nworld\" && done",
            Token::String(Loc { line: 1, column: 1 }, "hello\nworld"),
            Token::And(Loc { line: 2, column: 8 }),
            Token::Property(.., "done"),
        );
    }
}
