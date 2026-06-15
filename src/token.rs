use std::fmt::Display;

use super::location::Loc;
use super::operator::{BinaryOperator, LogicalOperator, UnaryOperator};

/// A lexical token produced by the scanner.
///
/// Every variant carries the source [`Loc`] at which it was found. Tokens are
/// an internal detail of lexing and parsing: the operator tokens are converted
/// into the public [`BinaryOperator`], [`LogicalOperator`], and
/// [`UnaryOperator`] enums before being stored in the parsed expression tree.
#[derive(Debug, PartialEq)]
pub enum Token<'a> {
    /// An opening parenthesis `(`.
    LeftParen(Loc),
    /// A closing parenthesis `)`.
    RightParen(Loc),
    /// An opening bracket `[` (start of a tuple literal).
    LeftBracket(Loc),
    /// A closing bracket `]` (end of a tuple literal).
    RightBracket(Loc),
    /// A comma `,` separating tuple elements or function arguments.
    Comma(Loc),

    /// A property reference, carrying the property's name.
    Property(Loc, &'a str),

    /// The `null` literal.
    Null(Loc),
    /// The `true` literal.
    True(Loc),
    /// The `false` literal.
    False(Loc),
    /// A double-quoted string literal, carrying its (escape-processed) contents.
    String(Loc, &'a str),
    /// A raw string literal (`r"..."` or the hashed `r#"..."#` form), carrying
    /// its verbatim contents.
    RawString(Loc, &'a str),
    /// A numeric literal, carrying its source text.
    Number(Loc, &'a str),
    /// A duration literal such as `5m` or `1h30m`, carrying its source text.
    Duration(Loc, &'a str),

    /// The equality operator `==`.
    Equals(Loc),
    /// The inequality operator `!=`.
    NotEquals(Loc),
    /// The `contains` operator (case-insensitive).
    Contains(Loc),
    /// The `contains_cs` operator (case-sensitive).
    ContainsCs(Loc),
    /// The `in` operator (case-insensitive).
    In(Loc),
    /// The `in_cs` operator (case-sensitive).
    InCs(Loc),
    /// The `startswith` operator (case-insensitive).
    StartsWith(Loc),
    /// The `startswith_cs` operator (case-sensitive).
    StartsWithCs(Loc),
    /// The `endswith` operator (case-insensitive).
    EndsWith(Loc),
    /// The `endswith_cs` operator (case-sensitive).
    EndsWithCs(Loc),
    /// The `like` glob-match operator (case-insensitive).
    Like(Loc),
    /// The `like_cs` glob-match operator (case-sensitive).
    LikeCs(Loc),
    /// The `matches` regular-expression operator.
    Matches(Loc),
    /// The greater-than operator `>`.
    GreaterThan(Loc),
    /// The less-than operator `<`.
    SmallerThan(Loc),
    /// The greater-than-or-equal operator `>=`.
    GreaterEqual(Loc),
    /// The less-than-or-equal operator `<=`.
    SmallerEqual(Loc),

    /// The addition operator `+`.
    Plus(Loc),
    /// The subtraction operator `-`.
    Minus(Loc),

    /// The logical NOT operator `!`.
    Not(Loc),
    /// The logical AND operator `&&`.
    And(Loc),
    /// The logical OR operator `||`.
    Or(Loc),
}

impl Token<'_> {
    /// Returns the textual lexeme this token was parsed from (e.g. `"=="` for
    /// [`Token::Equals`], or the property name for [`Token::Property`]).
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
            Token::Duration(.., s) => s,

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

            Token::Plus(..) => "+",
            Token::Minus(..) => "-",

            Token::Not(..) => "!",
            Token::And(..) => "&&",
            Token::Or(..) => "||",
        }
    }

    /// Converts a binary-operator token into its public [`BinaryOperator`].
    ///
    /// # Panics
    ///
    /// Panics if called on a token which is not a binary operator. The parser
    /// only calls this on tokens it has already confirmed, so this never
    /// happens in practice.
    pub fn as_binary_operator(&self) -> BinaryOperator {
        match self {
            Token::Equals(..) => BinaryOperator::Equals,
            Token::NotEquals(..) => BinaryOperator::NotEquals,
            Token::GreaterThan(..) => BinaryOperator::GreaterThan,
            Token::SmallerThan(..) => BinaryOperator::SmallerThan,
            Token::GreaterEqual(..) => BinaryOperator::GreaterEqual,
            Token::SmallerEqual(..) => BinaryOperator::SmallerEqual,
            Token::Contains(..) => BinaryOperator::Contains,
            Token::ContainsCs(..) => BinaryOperator::ContainsCs,
            Token::In(..) => BinaryOperator::In,
            Token::InCs(..) => BinaryOperator::InCs,
            Token::StartsWith(..) => BinaryOperator::StartsWith,
            Token::StartsWithCs(..) => BinaryOperator::StartsWithCs,
            Token::EndsWith(..) => BinaryOperator::EndsWith,
            Token::EndsWithCs(..) => BinaryOperator::EndsWithCs,
            Token::Plus(..) => BinaryOperator::Plus,
            Token::Minus(..) => BinaryOperator::Minus,
            other => unreachable!("token '{other}' is not a binary operator"),
        }
    }

    /// Converts a logical-operator token into its public [`LogicalOperator`].
    ///
    /// # Panics
    ///
    /// Panics if called on a token which is not `&&` or `||`. The parser only
    /// calls this on tokens it has already confirmed, so this never happens in
    /// practice.
    pub fn as_logical_operator(&self) -> LogicalOperator {
        match self {
            Token::And(..) => LogicalOperator::And,
            Token::Or(..) => LogicalOperator::Or,
            other => unreachable!("token '{other}' is not a logical operator"),
        }
    }

    /// Converts a unary-operator token into its public [`UnaryOperator`].
    ///
    /// # Panics
    ///
    /// Panics if called on a token which is not `!`. The parser only calls this
    /// on tokens it has already confirmed, so this never happens in practice.
    pub fn as_unary_operator(&self) -> UnaryOperator {
        match self {
            Token::Not(..) => UnaryOperator::Not,
            other => unreachable!("token '{other}' is not a unary operator"),
        }
    }

    /// Returns the source [`Loc`] at which this token appears.
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
            Token::Duration(loc, ..) => *loc,

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

            Token::Plus(loc) => *loc,
            Token::Minus(loc) => *loc,

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
            Token::RawString(.., s) => {
                // The token stores only the content, not how many `#` the author
                // used, so pick the fewest delimiters that can hold it (none
                // unless the content itself contains a `"`).
                let hashes = "#".repeat(required_raw_string_hashes(s));
                write!(f, "r{hashes}\"{s}\"{hashes}")
            }
            t => write!(f, "{}", t.lexeme()),
        }
    }
}

/// Returns the smallest number of `#` delimiters needed to write `content` as a
/// raw string without it terminating early: `0` when the content has no `"`, and
/// otherwise one more than the longest run of `#` immediately following a `"`.
fn required_raw_string_hashes(content: &str) -> usize {
    let bytes = content.as_bytes();
    let mut hashes = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'"' {
            let run = bytes[i + 1..].iter().take_while(|&&b| b == b'#').count();
            hashes = hashes.max(run + 1);
        }
    }
    hashes
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
    #[case(Token::Duration(LOC, "1h30m"), "1h30m")]
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
    #[case(Token::Plus(LOC), "+")]
    #[case(Token::Minus(LOC), "-")]
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
    // Content with a bare quote needs the hashed form so it can round-trip...
    #[case(Token::RawString(LOC, "{\"k\":1}"), "r#\"{\"k\":1}\"#")]
    // ...and content containing `"#` needs an extra hash to stay unambiguous.
    #[case(Token::RawString(LOC, "a\"#b"), "r##\"a\"#b\"##")]
    #[case(Token::And(LOC), "&&")]
    fn display_matches_lexeme(#[case] token: Token<'_>, #[case] expected: &str) {
        assert_eq!(token.to_string(), expected);
    }
}
