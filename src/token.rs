use std::fmt::Display;

use super::location::Loc;

#[derive(Debug, PartialEq)]
pub enum Token<'a> {
    LeftParen(Loc),
    RightParen(Loc),
    LeftBracket(Loc),
    RightBracket(Loc),
    Comma(Loc),

    Property(Loc, &'a str),

    Null(Loc),
    True(Loc),
    False(Loc),
    String(Loc, &'a str),
    RawString(Loc, &'a str),
    Number(Loc, &'a str),

    Equals(Loc),
    NotEquals(Loc),
    Contains(Loc),
    ContainsCs(Loc),
    In(Loc),
    InCs(Loc),
    StartsWith(Loc),
    StartsWithCs(Loc),
    EndsWith(Loc),
    EndsWithCs(Loc),
    Like(Loc),
    LikeCs(Loc),
    Matches(Loc),
    GreaterThan(Loc),
    SmallerThan(Loc),
    GreaterEqual(Loc),
    SmallerEqual(Loc),

    Not(Loc),
    And(Loc),
    Or(Loc),
}

impl Token<'_> {
    pub fn lexeme(&self) -> &str {
        match self {
            Token::LeftParen(..) => "(",
            Token::RightParen(..) => ")",
            Token::LeftBracket(..) => "[",
            Token::RightBracket(..) => "]",
            Token::Comma(..) => ",",

            Token::Property(.., s) => s,

            Token::Null(..) => "null",
            Token::True(..) => "true",
            Token::False(..) => "false",
            Token::String(.., s) => s,
            Token::RawString(.., s) => s,
            Token::Number(.., s) => s,

            Token::Equals(..) => "==",
            Token::NotEquals(..) => "!=",
            Token::Contains(..) => "contains",
            Token::ContainsCs(..) => "contains_cs",
            Token::In(..) => "in",
            Token::InCs(..) => "in_cs",
            Token::StartsWith(..) => "startswith",
            Token::StartsWithCs(..) => "startswith_cs",
            Token::EndsWith(..) => "endswith",
            Token::EndsWithCs(..) => "endswith_cs",
            Token::Like(..) => "like",
            Token::LikeCs(..) => "like_cs",
            Token::Matches(..) => "matches",
            Token::GreaterThan(..) => ">",
            Token::GreaterEqual(..) => ">=",
            Token::SmallerThan(..) => "<",
            Token::SmallerEqual(..) => "<=",

            Token::Not(..) => "!",
            Token::And(..) => "&&",
            Token::Or(..) => "||",
        }
    }

    pub fn location(&self) -> Loc {
        match self {
            Token::LeftParen(loc) => *loc,
            Token::RightParen(loc) => *loc,
            Token::LeftBracket(loc) => *loc,
            Token::RightBracket(loc) => *loc,
            Token::Comma(loc) => *loc,

            Token::Property(loc, ..) => *loc,

            Token::Null(loc) => *loc,
            Token::True(loc) => *loc,
            Token::False(loc) => *loc,
            Token::String(loc, ..) => *loc,
            Token::RawString(loc, ..) => *loc,
            Token::Number(loc, ..) => *loc,

            Token::Equals(loc) => *loc,
            Token::NotEquals(loc) => *loc,
            Token::Contains(loc) => *loc,
            Token::ContainsCs(loc) => *loc,
            Token::In(loc) => *loc,
            Token::InCs(loc) => *loc,
            Token::StartsWith(loc) => *loc,
            Token::StartsWithCs(loc) => *loc,
            Token::EndsWith(loc) => *loc,
            Token::EndsWithCs(loc) => *loc,
            Token::Like(loc) => *loc,
            Token::LikeCs(loc) => *loc,
            Token::Matches(loc) => *loc,
            Token::GreaterThan(loc) => *loc,
            Token::SmallerThan(loc) => *loc,
            Token::GreaterEqual(loc) => *loc,
            Token::SmallerEqual(loc) => *loc,

            Token::Not(loc) => *loc,
            Token::And(loc) => *loc,
            Token::Or(loc) => *loc,
        }
    }
}

impl Display for Token<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::String(.., s) => write!(f, "\"{s}\""),
            Token::RawString(.., s) => write!(f, "r\"{s}\""),
            t => write!(f, "{}", t.lexeme()),
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    const LOC: Loc = Loc { line: 3, column: 7 };

    #[rstest]
    #[case(Token::LeftParen(LOC), "(")]
    #[case(Token::RightParen(LOC), ")")]
    #[case(Token::LeftBracket(LOC), "[")]
    #[case(Token::RightBracket(LOC), "]")]
    #[case(Token::Comma(LOC), ",")]
    #[case(Token::Property(LOC, "repo.name"), "repo.name")]
    #[case(Token::Null(LOC), "null")]
    #[case(Token::True(LOC), "true")]
    #[case(Token::False(LOC), "false")]
    #[case(Token::String(LOC, "hello"), "hello")]
    #[case(Token::RawString(LOC, "he\\llo"), "he\\llo")]
    #[case(Token::Number(LOC, "1.5"), "1.5")]
    #[case(Token::Equals(LOC), "==")]
    #[case(Token::NotEquals(LOC), "!=")]
    #[case(Token::Contains(LOC), "contains")]
    #[case(Token::ContainsCs(LOC), "contains_cs")]
    #[case(Token::In(LOC), "in")]
    #[case(Token::InCs(LOC), "in_cs")]
    #[case(Token::StartsWith(LOC), "startswith")]
    #[case(Token::StartsWithCs(LOC), "startswith_cs")]
    #[case(Token::EndsWith(LOC), "endswith")]
    #[case(Token::EndsWithCs(LOC), "endswith_cs")]
    #[case(Token::Like(LOC), "like")]
    #[case(Token::LikeCs(LOC), "like_cs")]
    #[case(Token::Matches(LOC), "matches")]
    #[case(Token::GreaterThan(LOC), ">")]
    #[case(Token::GreaterEqual(LOC), ">=")]
    #[case(Token::SmallerThan(LOC), "<")]
    #[case(Token::SmallerEqual(LOC), "<=")]
    #[case(Token::Not(LOC), "!")]
    #[case(Token::And(LOC), "&&")]
    #[case(Token::Or(LOC), "||")]
    fn lexemes_and_locations(#[case] token: Token<'_>, #[case] lexeme: &str) {
        assert_eq!(token.lexeme(), lexeme);
        assert_eq!(token.location(), LOC);
    }

    #[rstest]
    #[case(Token::Property(LOC, "repo.name"), "repo.name")]
    #[case(Token::Number(LOC, "1.5"), "1.5")]
    #[case(Token::String(LOC, "hello"), "\"hello\"")]
    #[case(Token::RawString(LOC, "^\\d+$"), "r\"^\\d+$\"")]
    #[case(Token::And(LOC), "&&")]
    fn display_matches_lexeme(#[case] token: Token<'_>, #[case] expected: &str) {
        assert_eq!(token.to_string(), expected);
    }
}
