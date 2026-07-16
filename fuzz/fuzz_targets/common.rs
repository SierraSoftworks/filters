#![allow(dead_code)]

use arbitrary::{Arbitrary, Unstructured};

const MAX_INPUT_LENGTH: usize = 16 * 1024;

pub fn bounded_bytes(data: &[u8]) -> &[u8] {
    &data[..data.len().min(MAX_INPUT_LENGTH)]
}

pub fn bounded_text(data: &[u8]) -> String {
    String::from_utf8_lossy(bounded_bytes(data)).into_owned()
}

#[derive(Debug)]
pub enum FuzzInput {
    Raw(Vec<u8>),
    Structured(StructuredFilter),
    Generated(GeneratedFilter),
}

impl FuzzInput {
    pub fn decode(data: &[u8]) -> Self {
        let data = bounded_bytes(data);
        let Some((&selector, payload)) = data.split_first() else {
            return Self::Raw(Vec::new());
        };

        // The low bit selects between the free-form "raw" bytes path (even) and
        // one of the two grammar-aware generators (odd). The second bit then
        // chooses between the small hand-rolled `StructuredFilter` (bit clear)
        // and the recursive `GeneratedFilter` grammar (bit set), which together
        // reach a far wider range of operators, value types, functions, and
        // nesting than raw bytes alone tend to.
        if selector & 1 != 0 {
            let mut unstructured = Unstructured::new(payload);

            if selector & 2 != 0 {
                if let Ok(filter) = GeneratedFilter::arbitrary(&mut unstructured) {
                    return Self::Generated(filter);
                }
            } else if let Ok(filter) = StructuredFilter::arbitrary(&mut unstructured) {
                return Self::Structured(filter);
            }
        }

        Self::Raw(data.to_vec())
    }

    pub fn expression(&self) -> String {
        match self {
            Self::Raw(data) => bounded_text(data),
            Self::Structured(filter) => filter.expression(),
            Self::Generated(filter) => filter.expression(),
        }
    }

    pub fn is_deterministic(&self) -> bool {
        match self {
            Self::Raw(data) => expression_is_deterministic(&bounded_text(data)),
            Self::Structured(filter) => filter.function.is_deterministic(),
            Self::Generated(filter) => filter.is_deterministic(),
        }
    }
}

/// A filter is treated as deterministic (its result must not change between two
/// evaluations against the same target) unless it can consult the wall clock via
/// `now()` or `ago()`. The check is deliberately conservative: a literal that
/// merely contains the substring "now" or "ago" only costs us one skipped
/// equality assertion, never a false failure.
pub fn expression_is_deterministic(expression: &str) -> bool {
    !expression.contains("now") && !expression.contains("ago")
}

#[derive(Arbitrary, Debug)]
pub struct StructuredFilter {
    pub operator: Operator,
    pub function: BuiltInFunction,
    pub operand: Operand,
    pub text: String,
    pub number: f64,
    pub boolean: bool,
    pub tuple: Vec<String>,
}

impl StructuredFilter {
    pub fn expression(&self) -> String {
        let function = self.function.expression();
        let operand = self.operand.expression();

        match self.operator {
            Operator::Like => format!(r#"{function} like "*fuzz*""#),
            Operator::LikeCs => format!(r#"{function} like_cs "*fuzz*""#),
            Operator::Matches => format!(r#"{function} matches r"^.*fuzz.*$""#),
            Operator::And => format!("({function}) && ({operand})"),
            Operator::Or => format!("({function}) || ({operand})"),
            Operator::Not => format!("!({function})"),
            operator => format!("{function} {} {operand}", operator.symbol()),
        }
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum Operator {
    Equals,
    NotEquals,
    GreaterThan,
    SmallerThan,
    GreaterEqual,
    SmallerEqual,
    Contains,
    ContainsCs,
    In,
    InCs,
    StartsWith,
    StartsWithCs,
    EndsWith,
    EndsWithCs,
    Plus,
    Minus,
    Like,
    LikeCs,
    Matches,
    And,
    Or,
    Not,
}

impl Operator {
    pub const ALL: [Self; 22] = [
        Self::Equals,
        Self::NotEquals,
        Self::GreaterThan,
        Self::SmallerThan,
        Self::GreaterEqual,
        Self::SmallerEqual,
        Self::Contains,
        Self::ContainsCs,
        Self::In,
        Self::InCs,
        Self::StartsWith,
        Self::StartsWithCs,
        Self::EndsWith,
        Self::EndsWithCs,
        Self::Plus,
        Self::Minus,
        Self::Like,
        Self::LikeCs,
        Self::Matches,
        Self::And,
        Self::Or,
        Self::Not,
    ];

    fn symbol(self) -> &'static str {
        match self {
            Self::Equals => "==",
            Self::NotEquals => "!=",
            Self::GreaterThan => ">",
            Self::SmallerThan => "<",
            Self::GreaterEqual => ">=",
            Self::SmallerEqual => "<=",
            Self::Contains => "contains",
            Self::ContainsCs => "contains_cs",
            Self::In => "in",
            Self::InCs => "in_cs",
            Self::StartsWith => "startswith",
            Self::StartsWithCs => "startswith_cs",
            Self::EndsWith => "endswith",
            Self::EndsWithCs => "endswith_cs",
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Like | Self::LikeCs | Self::Matches | Self::And | Self::Or | Self::Not => {
                unreachable!("operator has specialized expression syntax")
            }
        }
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum BuiltInFunction {
    Trim,
    Now,
    Ago,
    DateTime,
}

impl BuiltInFunction {
    pub const ALL: [Self; 4] = [Self::Trim, Self::Now, Self::Ago, Self::DateTime];

    fn expression(self) -> &'static str {
        match self {
            Self::Trim => "trim(text)",
            Self::Now => "now()",
            Self::Ago => "ago(1m)",
            Self::DateTime => "datetime(text)",
        }
    }

    pub fn is_deterministic(self) -> bool {
        !matches!(self, Self::Now | Self::Ago)
    }
}

#[derive(Arbitrary, Clone, Copy, Debug)]
pub enum Operand {
    Text,
    Number,
    Boolean,
    Tuple,
    Missing,
    StringLiteral,
    NumberLiteral,
    BooleanLiteral,
    Null,
    Duration,
}

impl Operand {
    fn expression(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Boolean => "boolean",
            Self::Tuple => "tuple",
            Self::Missing => "missing",
            Self::StringLiteral => r#""fuzz""#,
            Self::NumberLiteral => "42.5",
            Self::BooleanLiteral => "true",
            Self::Null => "null",
            Self::Duration => "5m",
        }
    }
}

// ---------------------------------------------------------------------------
// Recursive, grammar-aware expression generator.
//
// `StructuredFilter` above pairs a single built-in call with one operator and
// one operand, which — while cheap and exhaustive over those axes — only ever
// produces a flat two-term expression. The generator below instead walks the
// crate's actual precedence grammar (logical → equality → comparison → term →
// unary → primary), so it can emit deeply nested expressions that mix every
// operator, literal type (strings, raw strings, numbers, durations, tuple
// literals, booleans, and null), function call, and `like`/`like_cs`/`matches`
// pattern. This reaches lexer, parser, and interpreter paths that the flat
// generator never touches.

/// Maximum recursion depth for the generated expression grammar. Each nested
/// group, unary operator, or function argument consumes one level, keeping both
/// the generated string and the parser's own recursion comfortably bounded.
const MAX_DEPTH: usize = 4;

/// Property names the generator references. A mix of the fields the evaluate
/// harness resolves, dotted paths, and deliberately-absent names (which resolve
/// to [`filt_rs::FilterValue::Null`]) so null-handling paths are exercised too.
const PROPERTIES: [&str; 10] = [
    "text",
    "number",
    "boolean",
    "tuple",
    "doc.title",
    "doc.pages",
    "doc.published",
    "doc.tags",
    "missing",
    "unknown.field",
];

/// Curated string-literal contents chosen to stress the lexer and comparison
/// helpers: empty strings, ASCII, Unicode with tricky case folding, embedded
/// quotes and newlines, and an RFC 3339 timestamp for the `datetime` function.
const STRING_CONTENTS: [&str; 16] = [
    "",
    "fuzz",
    "rust",
    "The Rust Book",
    "café",
    "groß",
    "Σ",
    "ς",
    "2026-03-12T12:00:00Z",
    "  padded  ",
    "a\"b",
    "line\nbreak",
    "*",
    "emoji 🎉",
    "null",
    "true",
];

/// Contents used for raw string literals, including regex-flavoured patterns and
/// text with embedded quotes/backslashes that a plain string could not hold.
const RAW_CONTENTS: [&str; 6] = [
    "^v\\d+$",
    "a\"b",
    "\\d+",
    "path/to/*",
    "café",
    "he said \"hi\"",
];

/// Glob patterns for the `like`/`like_cs` operators.
const GLOB_PATTERNS: [&str; 9] = [
    "*", "*fuzz*", "rust*", "*book", "?ust", "café*", "[abc]", "a*b*c", "",
];

/// Valid regular expressions for the `matches` operator (none contain a quote,
/// so they embed safely in a `r"..."` literal).
const REGEX_PATTERNS: [&str; 8] = [
    "^.*fuzz.*$",
    "\\d+",
    "^v\\d+(\\.\\d+){2}$",
    "[a-z]+",
    "(foo|bar)",
    ".",
    "^$",
    "\\w*",
];

/// Duration literals covering every unit and compound forms.
const DURATIONS: [&str; 9] = [
    "5m",
    "500ms",
    "1h30m",
    "2h",
    "7d",
    "1w",
    "1w2d3h4m5s6ms",
    "0s",
    "1.5h",
];

/// Relational, membership, prefix, and suffix operators (both case-insensitive
/// and case-sensitive variants).
const RELATIONAL: [&str; 12] = [
    ">",
    "<",
    ">=",
    "<=",
    "contains",
    "contains_cs",
    "in",
    "in_cs",
    "startswith",
    "startswith_cs",
    "endswith",
    "endswith_cs",
];

/// Names of functions that are *not* registered, used to exercise the parser's
/// unknown-function and arity error paths.
const UNKNOWN_FUNCTIONS: [&str; 4] = ["len", "upper", "lower", "size"];

/// A fuzz input carrying a recursively-generated filter expression alongside the
/// target field values it should be evaluated against.
#[derive(Debug)]
pub struct GeneratedFilter {
    expression: String,
    deterministic: bool,
    pub text: String,
    pub number: f64,
    pub boolean: bool,
    pub tuple: Vec<String>,
}

impl<'a> Arbitrary<'a> for GeneratedFilter {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let expression = generate_or(u, MAX_DEPTH)?;
        let deterministic = expression_is_deterministic(&expression);
        let text = u.arbitrary()?;
        let number = u.arbitrary()?;
        let boolean = u.arbitrary()?;
        let tuple = u.arbitrary()?;

        Ok(Self {
            expression,
            deterministic,
            text,
            number,
            boolean,
            tuple,
        })
    }
}

impl GeneratedFilter {
    pub fn expression(&self) -> String {
        self.expression.clone()
    }

    pub fn is_deterministic(&self) -> bool {
        self.deterministic
    }
}

/// Escapes a string so it lexes as a terminating `"..."` literal: embedded
/// quotes become `\"`, and any trailing backslashes (which would otherwise
/// escape the closing quote) are dropped.
fn escape_string(content: &str) -> String {
    let mut escaped = content.replace('"', "\\\"");
    while escaped.ends_with('\\') {
        escaped.pop();
    }
    escaped
}

/// Returns a `#` count large enough that `content` cannot contain the raw-string
/// terminator (`"` followed by that many `#`), by exceeding the longest run of
/// `#` anywhere in the content.
fn safe_raw_hashes(content: &str) -> usize {
    let mut longest = 0usize;
    let mut current = 0usize;
    for byte in content.bytes() {
        if byte == b'#' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    longest + 1
}

/// Reads a short arbitrary string (bounded to keep generated expressions small).
fn arbitrary_content(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let raw: String = u.arbitrary()?;
    Ok(raw.chars().take(48).collect())
}

fn generate_string_literal(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let content = if u.ratio(3, 4)? {
        (*u.choose(&STRING_CONTENTS)?).to_string()
    } else {
        arbitrary_content(u)?
    };
    Ok(format!("\"{}\"", escape_string(&content)))
}

fn generate_raw_string_literal(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let content = if u.ratio(1, 2)? {
        (*u.choose(&RAW_CONTENTS)?).to_string()
    } else {
        arbitrary_content(u)?
    };
    let pad = "#".repeat(safe_raw_hashes(&content));
    Ok(format!("r{pad}\"{content}\"{pad}"))
}

fn generate_number(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let whole: u16 = u.arbitrary()?;
    if u.ratio(1, 3)? {
        let fraction: u16 = u.arbitrary()?;
        Ok(format!("{whole}.{fraction}"))
    } else {
        Ok(format!("{whole}"))
    }
}

fn generate_scalar_literal(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    match u.int_in_range(0..=5)? {
        0 => generate_string_literal(u),
        1 => generate_raw_string_literal(u),
        2 => generate_number(u),
        3 => Ok(if u.arbitrary()? { "true" } else { "false" }.to_string()),
        4 => Ok("null".to_string()),
        _ => Ok((*u.choose(&DURATIONS)?).to_string()),
    }
}

fn generate_array(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let count = u.int_in_range(0..=4)?;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        items.push(generate_scalar_literal(u)?);
    }
    Ok(format!("[{}]", items.join(", ")))
}

fn generate_glob_literal(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    if u.ratio(1, 4)? {
        generate_raw_string_literal(u)
    } else {
        Ok(format!("\"{}\"", escape_string(u.choose(&GLOB_PATTERNS)?)))
    }
}

fn generate_regex_literal(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    if u.ratio(3, 4)? {
        Ok(format!("r\"{}\"", u.choose(&REGEX_PATTERNS)?))
    } else {
        // Arbitrary raw content may be an invalid regex, exercising the parser's
        // pattern-compilation error path.
        generate_raw_string_literal(u)
    }
}

fn generate_function_call(u: &mut Unstructured<'_>, depth: usize) -> arbitrary::Result<String> {
    if u.ratio(1, 8)? {
        let name = u.choose(&UNKNOWN_FUNCTIONS)?;
        return Ok(format!("{name}({})", generate_or(u, depth - 1)?));
    }

    match u.int_in_range(0..=3)? {
        0 => Ok(format!("trim({})", generate_or(u, depth - 1)?)),
        1 => Ok("now()".to_string()),
        2 => Ok(format!("ago({})", u.choose(&DURATIONS)?)),
        _ => {
            let argument = if u.ratio(1, 2)? {
                generate_string_literal(u)?
            } else {
                generate_or(u, depth - 1)?
            };
            Ok(format!("datetime({argument})"))
        }
    }
}

fn generate_leaf(u: &mut Unstructured<'_>, depth: usize) -> arbitrary::Result<String> {
    let can_call = depth > 0;
    let max = if can_call { 8 } else { 7 };
    match u.int_in_range(0..=max)? {
        0 => Ok((*u.choose(&PROPERTIES)?).to_string()),
        1 => generate_string_literal(u),
        2 => generate_raw_string_literal(u),
        3 => generate_number(u),
        4 => Ok(if u.arbitrary()? { "true" } else { "false" }.to_string()),
        5 => Ok("null".to_string()),
        6 => Ok((*u.choose(&DURATIONS)?).to_string()),
        7 => generate_array(u),
        _ => generate_function_call(u, depth),
    }
}

fn generate_primary(u: &mut Unstructured<'_>, depth: usize) -> arbitrary::Result<String> {
    if depth > 0 && u.int_in_range(0..=4)? == 0 {
        Ok(format!("({})", generate_or(u, depth - 1)?))
    } else {
        generate_leaf(u, depth)
    }
}

fn generate_unary(u: &mut Unstructured<'_>, depth: usize) -> arbitrary::Result<String> {
    if depth > 0 && u.ratio(1, 4)? {
        Ok(format!("!{}", generate_unary(u, depth - 1)?))
    } else {
        generate_primary(u, depth)
    }
}

fn generate_term(u: &mut Unstructured<'_>, depth: usize) -> arbitrary::Result<String> {
    let mut expression = generate_unary(u, depth)?;
    for _ in 0..u.int_in_range(0..=2)? {
        let operator = if u.arbitrary()? { "+" } else { "-" };
        expression = format!("{expression} {operator} {}", generate_unary(u, depth)?);
    }
    Ok(expression)
}

fn generate_comparison(u: &mut Unstructured<'_>, depth: usize) -> arbitrary::Result<String> {
    let left = generate_term(u, depth)?;
    match u.int_in_range(0..=3)? {
        0 => Ok(left),
        1 => {
            let operator = u.choose(&RELATIONAL)?;
            Ok(format!("{left} {operator} {}", generate_term(u, depth)?))
        }
        2 => {
            let keyword = if u.arbitrary()? { "like_cs" } else { "like" };
            Ok(format!("{left} {keyword} {}", generate_glob_literal(u)?))
        }
        _ => Ok(format!("{left} matches {}", generate_regex_literal(u)?)),
    }
}

fn generate_equality(u: &mut Unstructured<'_>, depth: usize) -> arbitrary::Result<String> {
    let left = generate_comparison(u, depth)?;
    if u.ratio(1, 2)? {
        let operator = if u.arbitrary()? { "==" } else { "!=" };
        Ok(format!(
            "{left} {operator} {}",
            generate_comparison(u, depth)?
        ))
    } else {
        Ok(left)
    }
}

fn generate_and(u: &mut Unstructured<'_>, depth: usize) -> arbitrary::Result<String> {
    let mut expression = generate_equality(u, depth)?;
    for _ in 0..u.int_in_range(0..=2)? {
        expression = format!("{expression} && {}", generate_equality(u, depth)?);
    }
    Ok(expression)
}

fn generate_or(u: &mut Unstructured<'_>, depth: usize) -> arbitrary::Result<String> {
    let mut expression = generate_and(u, depth)?;
    for _ in 0..u.int_in_range(0..=2)? {
        expression = format!("{expression} || {}", generate_and(u, depth)?);
    }
    Ok(expression)
}
