use std::borrow::Cow;

#[cfg(feature = "regex")]
use super::pattern::CompiledRegex;
use super::{
    FilterValue, Filterable,
    expr::{Expr, ExprVisitor},
    pattern::Glob,
    token::Token,
};

/// Applies a string predicate to a value following the leniency rules of the
/// pattern-matching operators: strings are tested directly, tuples match when
/// any of their (directly nested) string elements match, and every other kind
/// of value never matches.
fn match_string_value<F: Fn(&str) -> bool>(value: &FilterValue, predicate: F) -> bool {
    match value {
        FilterValue::String(s) => predicate(s),
        FilterValue::Tuple(items) => items
            .iter()
            .any(|item| matches!(item, FilterValue::String(s) if predicate(s))),
        _ => false,
    }
}

pub struct FilterContext<'a, T: Filterable> {
    target: &'a T,
}

impl<'a, T: Filterable> FilterContext<'a, T> {
    pub fn new(target: &'a T) -> Self {
        Self { target }
    }
}

/// The interpreter produces `Cow<'e, FilterValue>` values which borrow from
/// the filter's AST wherever possible. This means that evaluating a filter
/// does not allocate for literal values (including string and tuple
/// literals); the only owned values are those produced by
/// [`Filterable::get`] when a property is resolved.
impl<'e, T: Filterable> ExprVisitor<'e, Cow<'e, FilterValue>> for FilterContext<'_, T> {
    fn visit_literal(&mut self, value: &'e FilterValue) -> Cow<'e, FilterValue> {
        Cow::Borrowed(value)
    }

    fn visit_property(&mut self, name: &'e str) -> Cow<'e, FilterValue> {
        Cow::Owned(self.target.get(name))
    }

    fn visit_binary(
        &mut self,
        left: &'e Expr<'e>,
        operator: &'e Token<'e>,
        right: &'e Expr<'e>,
    ) -> Cow<'e, FilterValue> {
        let left = self.visit_expr(left);
        let right = self.visit_expr(right);

        // NOTE: We compare through `as_ref()` to ensure that we invoke
        // `FilterValue`'s own comparison methods (which have bespoke
        // `lt`/`le`/`gt`/`ge` semantics) rather than `Cow`'s defaults,
        // which would derive them from `partial_cmp` instead.
        let left = left.as_ref();
        let right = right.as_ref();
        let result = match operator {
            Token::Equals(..) => left == right,
            Token::NotEquals(..) => left != right,
            Token::Contains(..) => left.contains(right),
            Token::ContainsCs(..) => left.contains_cs(right),
            Token::In(..) => right.contains(left),
            Token::InCs(..) => right.contains_cs(left),
            Token::StartsWith(..) => left.startswith(right),
            Token::StartsWithCs(..) => left.startswith_cs(right),
            Token::EndsWith(..) => left.endswith(right),
            Token::EndsWithCs(..) => left.endswith_cs(right),
            Token::GreaterThan(..) => left.gt(right),
            Token::SmallerThan(..) => left.lt(right),
            Token::GreaterEqual(..) => left.ge(right),
            Token::SmallerEqual(..) => left.le(right),
            token => unreachable!("Encountered an unexpected binary operator '{token}'"),
        };

        Cow::Owned(FilterValue::Bool(result))
    }

    fn visit_logical(
        &mut self,
        left: &'e Expr<'e>,
        operator: &'e Token<'e>,
        right: &'e Expr<'e>,
    ) -> Cow<'e, FilterValue> {
        let left = self.visit_expr(left);

        match operator {
            Token::And(..) if left.is_truthy() => self.visit_expr(right),
            Token::And(..) => left,
            Token::Or(..) if !left.is_truthy() => self.visit_expr(right),
            Token::Or(..) => left,
            token => unreachable!("Encountered an unexpected logical operator '{token}'"),
        }
    }

    fn visit_unary(
        &mut self,
        operator: &'e Token<'e>,
        right: &'e Expr<'e>,
    ) -> Cow<'e, FilterValue> {
        let right = self.visit_expr(right);

        match operator {
            Token::Not(..) => Cow::Owned(FilterValue::Bool(!right.is_truthy())),
            token => unreachable!("Encountered an unexpected unary operator '{token}'"),
        }
    }

    fn visit_like(&mut self, left: &'e Expr<'e>, glob: &'e Glob) -> Cow<'e, FilterValue> {
        let left = self.visit_expr(left);
        Cow::Owned(FilterValue::Bool(match_string_value(left.as_ref(), |s| {
            glob.is_match(s)
        })))
    }

    #[cfg(feature = "regex")]
    fn visit_matches(
        &mut self,
        left: &'e Expr<'e>,
        regex: &'e CompiledRegex,
    ) -> Cow<'e, FilterValue> {
        let left = self.visit_expr(left);
        Cow::Owned(FilterValue::Bool(match_string_value(left.as_ref(), |s| {
            regex.is_match(s)
        })))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::lexer::Scanner;

    use super::*;

    #[derive(Debug, PartialEq)]
    struct TestFilterable;

    impl TestFilterable {
        pub fn matches(filter: &str) -> bool {
            use crate::parser::Parser;

            let tokens = Scanner::new(filter);
            let expr = Parser::parse(tokens).expect("parse the filter");
            let mut context = FilterContext::new(&Self);
            let result = context.visit_expr(&expr);
            result.is_truthy()
        }
    }

    impl Filterable for TestFilterable {
        fn get(&self, property: &str) -> FilterValue {
            match property {
                "boolean" => true.into(),
                "string" => "Alice".into(),
                "number" => 1.into(),
                "null" => FilterValue::Null,
                "tuple" => vec![true.into(), false.into()].into(),
                "names" => vec!["Alice".into(), "Bob".into()].into(),
                "mixed" => vec![1.into(), "Bob".into(), FilterValue::Null].into(),
                _ => FilterValue::Null,
            }
        }
    }

    #[rstest]
    #[case("true", true)]
    #[case("false", false)]
    #[case("null", false)]
    #[case("1", true)]
    #[case("0", false)]
    #[case("\"\"", false)]
    #[case("\"Alice\"", true)]
    fn literals(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("boolean", true)]
    #[case("string", true)]
    #[case("number", true)]
    #[case("tuple", true)]
    #[case("null", false)]
    #[case("unknown", false)]
    fn properties(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("boolean == true", true)]
    #[case("boolean == false", false)]
    #[case("string == \"Alice\"", true)]
    #[case("string == \"Bob\"", false)]
    #[case("number == 1", true)]
    #[case("number == 2", false)]
    #[case("tuple == [true, false]", true)]
    #[case("tuple == [false, true]", false)]
    #[case("tuple == []", false)]
    #[case("null == null", true)]
    fn equals(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("2 > 1", true)]
    #[case("1 > 2", false)]
    #[case("2 >= 1", true)]
    #[case("2 >= 2", true)]
    fn greater_than(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("1 < 2", true)]
    #[case("2 < 1", false)]
    #[case("1 <= 2", true)]
    #[case("1 <= 1", true)]
    #[case("2 <= 1", false)]
    fn smaller(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("boolean != true", false)]
    #[case("boolean != false", true)]
    #[case("string != \"Alice\"", false)]
    #[case("string != \"Bob\"", true)]
    #[case("number != 1", false)]
    #[case("number != 2", true)]
    #[case("tuple != [true, false]", false)]
    #[case("tuple != [false, true]", true)]
    #[case("tuple != []", true)]
    #[case("null != null", false)]
    fn not_equals(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("string contains \"Ali\"", true)]
    #[case("string contains \"Bob\"", false)]
    #[case("tuple contains true", true)]
    #[case("tuple contains false", true)]
    #[case("tuple contains null", false)]
    #[case("null contains null", false)]
    fn contains(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("string in \"Alice\"", true)]
    #[case("\"Ali\" in string", true)]
    #[case("string in \"Bob\"", false)]
    #[case("\"Bob\" in string", false)]
    #[case("true in tuple", true)]
    #[case("false in tuple", true)]
    #[case("null in tuple", false)]
    #[case("number in 1", false)]
    #[case("null in null", false)]
    fn in_(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("string startswith \"Ali\"", true)]
    #[case("string startswith \"Bob\"", false)]
    #[case("string startswith null", false)]
    #[case("null startswith null", false)]
    fn startswith(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    // The `_cs` operators behave like their counterparts, but compare
    // strings case-sensitively.
    #[case("string contains_cs \"Ali\"", true)]
    #[case("string contains_cs \"ali\"", false)]
    #[case("string contains \"ali\"", true)]
    #[case("tuple contains_cs true", true)]
    #[case("null contains_cs null", false)]
    #[case("\"Ali\" in_cs string", true)]
    #[case("\"ali\" in_cs string", false)]
    #[case("true in_cs tuple", true)]
    #[case("string startswith_cs \"Ali\"", true)]
    #[case("string startswith_cs \"ali\"", false)]
    #[case("string startswith \"ali\"", true)]
    #[case("string startswith_cs null", false)]
    #[case("string endswith_cs \"ice\"", true)]
    #[case("string endswith_cs \"ICE\"", false)]
    #[case("string endswith \"ICE\"", true)]
    #[case("null endswith_cs null", false)]
    fn case_sensitive_operators(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("string endswith \"ce\"", true)]
    #[case("string endswith \"ob\"", false)]
    #[case("string endswith null", false)]
    #[case("null endswith null", false)]
    fn endswith(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    // Strings match the glob pattern (case-insensitively).
    #[case("string like \"Alice\"", true)]
    #[case("string like \"alice\"", true)]
    #[case("string like \"Al*\"", true)]
    #[case("string like \"al*\"", true)]
    #[case("string like \"*ice\"", true)]
    #[case("string like \"?lice\"", true)]
    #[case("string like \"*li*\"", true)]
    #[case("string like \"Bob*\"", false)]
    #[case("string like \"Alice?\"", false)]
    #[case("string like \"*\"", true)]
    #[case("string like \"\"", false)]
    // Tuples match when any of their string elements match.
    #[case("names like \"a*\"", true)]
    #[case("names like \"b?b\"", true)]
    #[case("names like \"carol\"", false)]
    #[case("mixed like \"bob\"", true)]
    #[case("mixed like \"1\"", false)] // numbers within tuples are not matched
    #[case("tuple like \"*\"", false)] // no string elements at all
    // Every other kind of value never matches, even against "*".
    #[case("boolean like \"*\"", false)]
    #[case("number like \"*\"", false)]
    #[case("null like \"*\"", false)]
    #[case("unknown like \"*\"", false)]
    // Literal left-hand sides work too.
    #[case("\"feat/login\" like \"feat/*\"", true)]
    #[case("\"fix/login\" like \"feat/*\"", false)]
    fn like(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[cfg(feature = "regex")]
    #[rstest]
    // Unanchored and case-sensitive by default.
    #[case("string matches r\"lic\"", true)]
    #[case("string matches r\"LIC\"", false)]
    #[case("string matches r\"(?i)LIC\"", true)]
    #[case("string matches r\"^Alice$\"", true)]
    #[case("string matches r\"^lice$\"", false)]
    #[case("string matches r\"^A\\w+$\"", true)]
    // Plain strings work as patterns too, with standard escape processing.
    #[case("string matches \"^A\\\\w+$\"", true)]
    // Tuples match when any of their string elements match.
    #[case("names matches r\"^B\\w+$\"", true)]
    #[case("names matches r\"^C\\w+$\"", false)]
    #[case("mixed matches r\"Bob\"", true)]
    #[case("mixed matches r\"^1$\"", false)]
    // Every other kind of value never matches.
    #[case("boolean matches r\".*\"", false)]
    #[case("number matches r\".*\"", false)]
    #[case("null matches r\".*\"", false)]
    // Literal left-hand sides work too.
    #[case("\"release/v1.2.3\" matches r\"^release/v\\d+(\\.\\d+){2}$\"", true)]
    #[case("\"release/v1.2\" matches r\"^release/v\\d+(\\.\\d+){2}$\"", false)]
    fn matches_regex(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("!boolean", false)]
    #[case("!string", false)]
    #[case("!number", false)]
    #[case("!tuple", false)]
    #[case("!null", true)]
    #[case("!!boolean", true)]
    fn not(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("true && true", true)]
    #[case("true && false", false)]
    #[case("false && true", false)]
    #[case("false && false", false)]
    #[case("string && number", true)]
    #[case("string && null", false)]
    fn and(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("true || true", true)]
    #[case("true || false", true)]
    #[case("false || true", true)]
    #[case("false || false", false)]
    #[case("string || number", true)]
    #[case("string || null", true)]
    #[case("null || null", false)]
    fn or(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("true && (false || true)", true)]
    #[case("true && (false || false)", false)]
    #[case("true && (string || null)", true)]
    #[case("false && (string || null)", false)]
    fn grouping(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("true && false || true", true)]
    #[case("true && false || false", false)]
    #[case("false && true || false", false)]
    #[case("false && false || true", true)]
    fn precedence(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[rstest]
    #[case("number > null", false)]
    #[case("number < null", false)]
    #[case("number >= null", false)]
    #[case("number <= null", false)]
    #[case("string > number", false)]
    fn mismatched_type_comparisons(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }
}
