//! Collect every property referenced by a filter expression.
//!
//! This example implements a custom [`ExprVisitor`] which walks a filter's
//! parsed [`Expr`] tree and records the name of every property it touches —
//! the kind of static analysis you might use to validate a user-supplied
//! filter against a known set of fields, or to decide which data you need to
//! load before evaluating it.
//!
//! Run it with:
//!
//! ```sh
//! cargo run --example property_collector
//! ```

use std::collections::BTreeSet;

use filt_rs::{
    BinaryOperator, Expr, ExprVisitor, Filter, FilterValue, Function, Glob, LogicalOperator,
    UnaryOperator,
};

/// An [`ExprVisitor`] which accumulates the names of all accessed properties.
///
/// The visitor borrows each property name straight out of the expression tree
/// (note the shared `'a` lifetime), so collecting them costs no allocations
/// beyond growing the set itself. The `()` result type means each `visit_*`
/// method simply records into `self` and recurses, rather than folding a value
/// back up the tree.
#[derive(Default)]
struct PropertyCollector<'a> {
    properties: BTreeSet<&'a str>,
}

impl<'a> ExprVisitor<'a, ()> for PropertyCollector<'a> {
    fn visit_literal(&mut self, _value: &'a FilterValue<'a>) {}

    fn visit_property(&mut self, name: &'a str) {
        self.properties.insert(name);
    }

    fn visit_function_call(&mut self, _function: &'a dyn Function, args: &'a [Expr<'a>]) {
        // A property may be passed as a function argument, so recurse into each.
        for arg in args {
            self.visit_expr(arg);
        }
    }

    fn visit_binary(&mut self, left: &'a Expr<'a>, _operator: BinaryOperator, right: &'a Expr<'a>) {
        self.visit_expr(left);
        self.visit_expr(right);
    }

    fn visit_logical(
        &mut self,
        left: &'a Expr<'a>,
        _operator: LogicalOperator,
        right: &'a Expr<'a>,
    ) {
        self.visit_expr(left);
        self.visit_expr(right);
    }

    fn visit_unary(&mut self, _operator: UnaryOperator, right: &'a Expr<'a>) {
        self.visit_expr(right);
    }

    fn visit_like(&mut self, left: &'a Expr<'a>, _glob: &'a Glob) {
        self.visit_expr(left);
    }

    #[cfg(feature = "regex")]
    fn visit_matches(&mut self, left: &'a Expr<'a>, _regex: &'a filt_rs::CompiledRegex) {
        self.visit_expr(left);
    }
}

fn main() -> Result<(), filt_rs::Error> {
    let expressions = [
        r#"repo.public && repo.stars >= 50"#,
        r#"repo.name startswith "git" || owner.login == "SierraSoftworks""#,
        r#"!repo.fork && repo.name in ["git-tool", "grey"] && repo.stars > repo.forks"#,
        // Properties are discovered through function-call arguments too.
        r#"trim(repo.description) != """#,
    ];

    for expression in expressions {
        let filter = Filter::new(expression)?;

        let mut collector = PropertyCollector::default();
        filter.visit(&mut collector);

        println!("Filter:     {}", filter.raw());
        println!(
            "Properties: {}\n",
            collector
                .properties
                .iter()
                .copied()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    Ok(())
}
