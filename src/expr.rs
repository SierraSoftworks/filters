use std::fmt::{Debug, Display};

#[cfg(feature = "regex")]
use super::pattern::CompiledRegex;
use super::{
    FilterValue,
    operator::{BinaryOperator, LogicalOperator, UnaryOperator},
    pattern::Glob,
};

/// A node in a parsed filter expression tree.
///
/// `Expr` is the abstract syntax tree produced when a [`Filter`](crate::Filter)
/// is parsed. Most users never touch it directly, but it is exposed so that
/// downstream crates can implement their own [`ExprVisitor`] and walk a
/// filter's structure via [`Filter::visit`](crate::Filter::visit) — for
/// example to extract the set of properties a filter references, or to
/// translate a filter into another query language.
///
/// The `'a` lifetime ties borrowed lexemes (property names, function names,
/// and string literals) to the original filter expression text.
// WARNING: We cannot have clone/copy semantics here because the [`Filter`] relies on
// pinning pointers to ensure that this struct can be safely used without additional
// allocations.
#[derive(PartialEq)]
pub enum Expr<'a> {
    /// A literal value such as `42`, `"text"`, `true`, or `["a", "b"]`.
    Literal(FilterValue<'a>),
    /// A reference to a property (e.g. `repo.name`), resolved at evaluation
    /// time via [`Filterable::get`](crate::Filterable::get).
    Property(&'a str),
    /// A call to a built-in function, carrying the function name and the
    /// argument expressions passed to it.
    FunctionCall(&'a str, Vec<Expr<'a>>),
    /// A binary operation — comparison, membership, or arithmetic — joining two
    /// operands with the [`BinaryOperator`] between them.
    Binary(Box<Expr<'a>>, BinaryOperator, Box<Expr<'a>>),
    /// A short-circuiting logical operation (see [`LogicalOperator`]).
    Logical(Box<Expr<'a>>, LogicalOperator, Box<Expr<'a>>),
    /// A unary operation (see [`UnaryOperator`]).
    Unary(UnaryOperator, Box<Expr<'a>>),
    /// A `like` / `like_cs` glob match against the left-hand operand.
    // Pattern expressions store their pattern pre-compiled (the compiled forms
    // own their data, so they don't depend on the pinned filter string) which
    // allows them to be evaluated without any pattern-related allocation.
    Like(Box<Expr<'a>>, Glob),
    /// A `matches` regular-expression match against the left-hand operand.
    #[cfg(feature = "regex")]
    Matches(Box<Expr<'a>>, CompiledRegex),
}

/// A visitor over [`Expr`] trees.
///
/// Implement this trait to walk the parsed structure of a
/// [`Filter`](crate::Filter) and fold it into a value of type `T`, then run it
/// via [`Filter::visit`](crate::Filter::visit). The built-in interpreter is
/// itself an `ExprVisitor` (producing a `Cow<FilterValue>`), and the
/// `property_collector` example in the `examples/` directory shows a visitor
/// which gathers the names of every property a filter references.
///
/// [`visit_expr`](ExprVisitor::visit_expr) dispatches each node to the matching
/// `visit_*` method; its default implementation is usually what you want.
/// Composite nodes are handed their child [`Expr`]s so you can recurse into
/// them with `self.visit_expr(child)`.
///
/// The `'a` lifetime ties the visited nodes to the expression tree itself,
/// allowing visitors to produce values which *borrow* from the AST (for
/// example, the interpreter returns `Cow::Borrowed` for literal values rather
/// than cloning them on every evaluation, and the property collector borrows
/// property names straight out of the tree).
pub trait ExprVisitor<'a, T> {
    /// Dispatches `expr` to the appropriate `visit_*` method based on its kind.
    ///
    /// The default implementation matches on the node and forwards to the
    /// relevant method; override it only if you need to observe every node
    /// before it is dispatched.
    fn visit_expr(&mut self, expr: &'a Expr<'a>) -> T {
        match expr {
            Expr::Literal(value) => self.visit_literal(value),
            Expr::Property(name) => self.visit_property(name),
            Expr::FunctionCall(name, args) => self.visit_function_call(name, args),
            Expr::Binary(left, operator, right) => self.visit_binary(left, *operator, right),
            Expr::Logical(left, operator, right) => self.visit_logical(left, *operator, right),
            Expr::Unary(operator, right) => self.visit_unary(*operator, right),
            Expr::Like(left, glob) => self.visit_like(left, glob),
            #[cfg(feature = "regex")]
            Expr::Matches(left, regex) => self.visit_matches(left, regex),
        }
    }

    /// Visits a literal value node (e.g. `42`, `"text"`, `true`, `["a", "b"]`).
    fn visit_literal(&mut self, value: &'a FilterValue<'a>) -> T;
    /// Visits a property reference node, carrying the property's name.
    fn visit_property(&mut self, name: &'a str) -> T;
    /// Visits a function-call node, carrying the function name and its argument
    /// expressions (recurse into the arguments with [`visit_expr`](ExprVisitor::visit_expr)).
    fn visit_function_call(&mut self, name: &'a str, args: &'a [Expr<'a>]) -> T;
    /// Visits a binary operation node — comparison, membership, or arithmetic.
    fn visit_binary(
        &mut self,
        left: &'a Expr<'a>,
        operator: BinaryOperator,
        right: &'a Expr<'a>,
    ) -> T;
    /// Visits a short-circuiting logical node (see [`LogicalOperator`]).
    fn visit_logical(
        &mut self,
        left: &'a Expr<'a>,
        operator: LogicalOperator,
        right: &'a Expr<'a>,
    ) -> T;
    /// Visits a unary operation node (see [`UnaryOperator`]).
    fn visit_unary(&mut self, operator: UnaryOperator, right: &'a Expr<'a>) -> T;
    /// Visits a `like` / `like_cs` glob-match node.
    fn visit_like(&mut self, left: &'a Expr<'a>, glob: &'a Glob) -> T;
    /// Visits a `matches` regular-expression node.
    #[cfg(feature = "regex")]
    fn visit_matches(&mut self, left: &'a Expr<'a>, regex: &'a CompiledRegex) -> T;
}

impl Display for Expr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut printer = ExprPrinter(f);
        printer.visit_expr(self)?;
        Ok(())
    }
}

impl Debug for Expr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut printer = ExprPrinter(f);
        printer.visit_expr(self)?;
        Ok(())
    }
}

struct ExprPrinter<'a, 'b>(&'a mut std::fmt::Formatter<'b>);
impl<'e> ExprVisitor<'e, std::fmt::Result> for ExprPrinter<'_, '_> {
    fn visit_literal(&mut self, value: &'e FilterValue<'e>) -> std::fmt::Result {
        write!(self.0, "{}", value)
    }

    fn visit_property(&mut self, name: &'e str) -> std::fmt::Result {
        write!(self.0, "(property {})", name)
    }

    fn visit_function_call(&mut self, name: &'e str, args: &'e [Expr<'e>]) -> std::fmt::Result {
        write!(self.0, "(call {}", name)?;
        for arg in args {
            write!(self.0, " ")?;
            self.visit_expr(arg)?;
        }
        write!(self.0, ")")
    }

    fn visit_binary(
        &mut self,
        left: &'e Expr<'e>,
        operator: BinaryOperator,
        right: &'e Expr<'e>,
    ) -> std::fmt::Result {
        write!(self.0, "({operator} ")?;
        self.visit_expr(left)?;
        write!(self.0, " ")?;
        self.visit_expr(right)?;
        write!(self.0, ")")
    }

    fn visit_logical(
        &mut self,
        left: &'e Expr<'e>,
        operator: LogicalOperator,
        right: &'e Expr<'e>,
    ) -> std::fmt::Result {
        write!(self.0, "({operator} ")?;
        self.visit_expr(left)?;
        write!(self.0, " ")?;
        self.visit_expr(right)?;
        write!(self.0, ")")
    }

    fn visit_unary(&mut self, operator: UnaryOperator, right: &'e Expr<'e>) -> std::fmt::Result {
        write!(self.0, "{operator}")?;
        self.visit_expr(right)
    }

    fn visit_like(&mut self, left: &'e Expr<'e>, glob: &'e Glob) -> std::fmt::Result {
        let operator = if glob.is_case_sensitive() {
            "like_cs"
        } else {
            "like"
        };
        write!(self.0, "({operator} ")?;
        self.visit_expr(left)?;
        write!(self.0, " {})", glob)
    }

    #[cfg(feature = "regex")]
    fn visit_matches(&mut self, left: &'e Expr<'e>, regex: &'e CompiledRegex) -> std::fmt::Result {
        write!(self.0, "(matches ")?;
        self.visit_expr(left)?;
        write!(self.0, " {})", regex)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(Expr::Literal("value".into()), "\"value\"")]
    #[case(Expr::Property("test"), "(property test)")]
    #[case(
        Expr::Binary(
            Box::new(Expr::Literal("value".into())),
            BinaryOperator::In,
            Box::new(Expr::Property("test")),
        ),
        "(in \"value\" (property test))"
    )]
    #[case(
        Expr::Logical(
            Box::new(Expr::Literal("value".into())),
            LogicalOperator::And,
            Box::new(Expr::Property("test")),
        ),
        "(&& \"value\" (property test))"
    )]
    #[case(
        Expr::Unary(UnaryOperator::Not, Box::new(Expr::Property("test")),),
        "!(property test)"
    )]
    #[case(
        Expr::Like(Box::new(Expr::Property("branch.name")), Glob::compile("feat/*"),),
        "(like (property branch.name) \"feat/*\")"
    )]
    #[case(
        Expr::Like(Box::new(Expr::Property("branch.name")), Glob::compile_cs("feat/*"),),
        "(like_cs (property branch.name) \"feat/*\")"
    )]
    #[case(Expr::FunctionCall("now", vec![]), "(call now)")]
    #[case(
        Expr::FunctionCall("now", vec![Expr::Literal(1.into()), Expr::Property("test")]),
        "(call now 1 (property test))"
    )]
    fn expression_visualization(#[case] expr: Expr<'_>, #[case] view: &str) {
        assert_eq!(view, format!("{expr}"));
        assert_eq!(view, format!("{expr:?}"));
    }

    #[cfg(feature = "regex")]
    #[test]
    fn matches_expression_visualization() {
        let expr = Expr::Matches(
            Box::new(Expr::Property("branch.name")),
            CompiledRegex::compile("^release/v\\d+$").expect("compile the pattern"),
        );
        let view = "(matches (property branch.name) \"^release/v\\\\d+$\")";
        assert_eq!(view, format!("{expr}"));
        assert_eq!(view, format!("{expr:?}"));
    }
}
