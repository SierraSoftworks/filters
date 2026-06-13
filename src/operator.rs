use std::fmt::Display;

/// A binary operator joining the two operands of an
/// [`Expr::Binary`](crate::Expr::Binary) node — a comparison, membership test,
/// or arithmetic operation.
///
/// This enum lists exactly the operators which can appear in a binary node, so
/// an [`ExprVisitor`](crate::ExprVisitor) can match on it exhaustively without
/// a catch-all arm. The short-circuiting logical operators live in
/// [`LogicalOperator`] instead, and `like`/`like_cs`/`matches` are represented
/// as their own [`Expr`](crate::Expr) nodes (they carry a compiled pattern).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOperator {
    /// Equality, `==`.
    Equals,
    /// Inequality, `!=`.
    NotEquals,
    /// Greater-than, `>`.
    GreaterThan,
    /// Less-than, `<`.
    SmallerThan,
    /// Greater-than-or-equal, `>=`.
    GreaterEqual,
    /// Less-than-or-equal, `<=`.
    SmallerEqual,
    /// Substring / tuple membership, `contains` (case-insensitive).
    Contains,
    /// Substring / tuple membership, `contains_cs` (case-sensitive).
    ContainsCs,
    /// Inverse membership, `in` (case-insensitive).
    In,
    /// Inverse membership, `in_cs` (case-sensitive).
    InCs,
    /// Prefix test, `startswith` (case-insensitive).
    StartsWith,
    /// Prefix test, `startswith_cs` (case-sensitive).
    StartsWithCs,
    /// Suffix test, `endswith` (case-insensitive).
    EndsWith,
    /// Suffix test, `endswith_cs` (case-sensitive).
    EndsWithCs,
    /// Addition, `+`.
    Plus,
    /// Subtraction, `-`.
    Minus,
}

impl BinaryOperator {
    /// Returns the textual symbol or keyword this operator is written as in a
    /// filter expression (e.g. `"=="`, `"contains"`).
    pub fn symbol(&self) -> &'static str {
        match self {
            BinaryOperator::Equals => "==",
            BinaryOperator::NotEquals => "!=",
            BinaryOperator::GreaterThan => ">",
            BinaryOperator::SmallerThan => "<",
            BinaryOperator::GreaterEqual => ">=",
            BinaryOperator::SmallerEqual => "<=",
            BinaryOperator::Contains => "contains",
            BinaryOperator::ContainsCs => "contains_cs",
            BinaryOperator::In => "in",
            BinaryOperator::InCs => "in_cs",
            BinaryOperator::StartsWith => "startswith",
            BinaryOperator::StartsWithCs => "startswith_cs",
            BinaryOperator::EndsWith => "endswith",
            BinaryOperator::EndsWithCs => "endswith_cs",
            BinaryOperator::Plus => "+",
            BinaryOperator::Minus => "-",
        }
    }
}

impl Display for BinaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.symbol())
    }
}

/// A short-circuiting logical operator joining the two operands of an
/// [`Expr::Logical`](crate::Expr::Logical) node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogicalOperator {
    /// Logical AND, `&&` — evaluates the right operand only if the left is truthy.
    And,
    /// Logical OR, `||` — evaluates the right operand only if the left is falsy.
    Or,
}

impl LogicalOperator {
    /// Returns the textual symbol this operator is written as (`"&&"` or `"||"`).
    pub fn symbol(&self) -> &'static str {
        match self {
            LogicalOperator::And => "&&",
            LogicalOperator::Or => "||",
        }
    }
}

impl Display for LogicalOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.symbol())
    }
}

/// A unary operator applied to the single operand of an
/// [`Expr::Unary`](crate::Expr::Unary) node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOperator {
    /// Logical NOT, `!`.
    Not,
}

impl UnaryOperator {
    /// Returns the textual symbol this operator is written as (`"!"`).
    pub fn symbol(&self) -> &'static str {
        match self {
            UnaryOperator::Not => "!",
        }
    }
}

impl Display for UnaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.symbol())
    }
}
