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
}

impl FuzzInput {
    pub fn decode(data: &[u8]) -> Self {
        let data = bounded_bytes(data);
        let Some((&selector, payload)) = data.split_first() else {
            return Self::Raw(Vec::new());
        };

        if selector & 1 != 0 {
            let mut unstructured = Unstructured::new(payload);
            if let Ok(filter) = StructuredFilter::arbitrary(&mut unstructured) {
                return Self::Structured(filter);
            }
        }

        Self::Raw(data.to_vec())
    }

    pub fn expression(&self) -> String {
        match self {
            Self::Raw(data) => bounded_text(data),
            Self::Structured(filter) => filter.expression(),
        }
    }

    pub fn is_deterministic(&self) -> bool {
        match self {
            Self::Raw(data) => {
                let expression = bounded_text(data);
                !expression.contains("now") && !expression.contains("ago")
            }
            Self::Structured(filter) => filter.function.is_deterministic(),
        }
    }
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
