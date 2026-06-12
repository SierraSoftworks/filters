use std::fmt::{Debug, Display};

#[cfg(feature = "regex")]
use super::pattern::CompiledRegex;
use super::{FilterValue, pattern::Glob, token::Token};

// WARNING: We cannot have clone/copy semantics here because the [`Filter`] relies on
// pinning pointers to ensure that this struct can be safely used without additional
// allocations.
#[derive(PartialEq)]
pub enum Expr<'a> {
    Literal(FilterValue),
    Property(&'a str),
    FunctionCall(&'a str, Vec<Expr<'a>>),
    Binary(Box<Expr<'a>>, Token<'a>, Box<Expr<'a>>),
    Logical(Box<Expr<'a>>, Token<'a>, Box<Expr<'a>>),
    Unary(Token<'a>, Box<Expr<'a>>),
    // Pattern expressions store their pattern pre-compiled (the compiled forms
    // own their data, so they don't depend on the pinned filter string) which
    // allows them to be evaluated without any pattern-related allocation.
    Like(Box<Expr<'a>>, Glob),
    #[cfg(feature = "regex")]
    Matches(Box<Expr<'a>>, CompiledRegex),
}

/// A visitor over [`Expr`] trees.
///
/// The `'a` lifetime ties the visited nodes to the expression tree itself,
/// allowing visitors to produce values which *borrow* from the AST (for
/// example, the interpreter returns `Cow::Borrowed` for literal values
/// rather than cloning them on every evaluation).
pub trait ExprVisitor<'a, T> {
    fn visit_expr(&mut self, expr: &'a Expr<'a>) -> T {
        match expr {
            Expr::Literal(value) => self.visit_literal(value),
            Expr::Property(name) => self.visit_property(name),
            Expr::FunctionCall(name, args) => self.visit_function_call(name, args),
            Expr::Binary(left, operator, right) => self.visit_binary(left, operator, right),
            Expr::Logical(left, operator, right) => self.visit_logical(left, operator, right),
            Expr::Unary(operator, right) => self.visit_unary(operator, right),
            Expr::Like(left, glob) => self.visit_like(left, glob),
            #[cfg(feature = "regex")]
            Expr::Matches(left, regex) => self.visit_matches(left, regex),
        }
    }

    fn visit_literal(&mut self, value: &'a FilterValue) -> T;
    fn visit_property(&mut self, name: &'a str) -> T;
    fn visit_function_call(&mut self, name: &str, args: &[Expr]) -> T;
    fn visit_binary(
        &mut self,
        left: &'a Expr<'a>,
        operator: &'a Token<'a>,
        right: &'a Expr<'a>,
    ) -> T;
    fn visit_logical(
        &mut self,
        left: &'a Expr<'a>,
        operator: &'a Token<'a>,
        right: &'a Expr<'a>,
    ) -> T;
    fn visit_unary(&mut self, operator: &'a Token<'a>, right: &'a Expr<'a>) -> T;
    fn visit_like(&mut self, left: &'a Expr<'a>, glob: &'a Glob) -> T;
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
    fn visit_literal(&mut self, value: &'e FilterValue) -> std::fmt::Result {
        write!(self.0, "{}", value)
    }

    fn visit_property(&mut self, name: &'e str) -> std::fmt::Result {
        write!(self.0, "(property {})", name)
    }

    fn visit_function_call(&mut self, name: &str, args: &[Expr]) -> std::fmt::Result {
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
        operator: &'e Token<'e>,
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
        operator: &'e Token<'e>,
        right: &'e Expr<'e>,
    ) -> std::fmt::Result {
        write!(self.0, "({operator} ")?;
        self.visit_expr(left)?;
        write!(self.0, " ")?;
        self.visit_expr(right)?;
        write!(self.0, ")")
    }

    fn visit_unary(&mut self, operator: &'e Token<'e>, right: &'e Expr<'e>) -> std::fmt::Result {
        write!(self.0, "{}", operator.lexeme())?;
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

    use crate::location::Loc;

    use super::*;

    #[rstest]
    #[case(Expr::Literal("value".into()), "\"value\"")]
    #[case(Expr::Property("test"), "(property test)")]
    #[case(
        Expr::Binary(
            Box::new(Expr::Literal("value".into())),
            Token::In(Loc::new(1, 8)),
            Box::new(Expr::Property("test")),
        ),
        "(in \"value\" (property test))"
    )]
    #[case(
        Expr::Logical(
            Box::new(Expr::Literal("value".into())),
            Token::And(Loc::new(1, 8)),
            Box::new(Expr::Property("test")),
        ),
        "(&& \"value\" (property test))"
    )]
    #[case(
        Expr::Unary(Token::Not(Loc::new(1, 1)), Box::new(Expr::Property("test")),),
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
