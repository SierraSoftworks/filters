use std::borrow::Cow;

#[cfg(feature = "secrecy")]
use secrecy::ExposeSecret;

#[cfg(feature = "regex")]
use super::pattern::CompiledRegex;
use super::{
    FilterValue, Filterable,
    expr::{Expr, ExprVisitor},
    operator::{BinaryOperator, LogicalOperator, UnaryOperator},
    pattern::Glob,
};

/// Applies a string predicate to a value following the leniency rules of the
/// pattern-matching operators: strings are tested directly, tuples match when
/// any of their (directly nested) string elements match, and every other kind
/// of value never matches.
fn match_string_value<F: Fn(&str) -> bool>(value: &FilterValue<'_>, predicate: F) -> bool {
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
impl<'e, T: Filterable> ExprVisitor<'e, Cow<'e, FilterValue<'e>>> for FilterContext<'e, T> {
    fn visit_literal(&mut self, value: &'e FilterValue<'e>) -> Cow<'e, FilterValue<'e>> {
        Cow::Borrowed(value)
    }

    fn visit_property(&mut self, name: &'e str) -> Cow<'e, FilterValue<'e>> {
        // Copy the `&'e T` out of `&mut self` first so the borrow produced by
        // `get` is tied to the full target lifetime `'e` (and may therefore
        // borrow from the target), not to this short `&mut self` reborrow.
        let target: &'e T = self.target;
        Cow::Owned(target.get(name))
    }

    fn visit_function_call(
        &mut self,
        name: &'e str,
        args: &'e [Expr<'e>],
    ) -> Cow<'e, FilterValue<'e>> {
        match name {
            // now() is evaluated at filtering time, so each call to
            // Filter::matches sees the current time.
            #[cfg(feature = "chrono")]
            "now" => Cow::Owned(FilterValue::DateTime(chrono::Utc::now())),
            // trim() strips leading and trailing whitespace from a string,
            // borrowing the trimmed sub-slice wherever it can.
            "trim" => trim(self.visit_expr(&args[0])),
            // Function names and arities are validated when the filter is
            // parsed, so an unknown function here indicates a parser bug.
            _ => unreachable!("Encountered a call to an unexpected function '{name}'"),
        }
    }

    fn visit_binary(
        &mut self,
        left: &'e Expr<'e>,
        operator: BinaryOperator,
        right: &'e Expr<'e>,
    ) -> Cow<'e, FilterValue<'e>> {
        let left = self.visit_expr(left);
        let right = self.visit_expr(right);

        // NOTE: We compare through `as_ref()` to ensure that we invoke
        // `FilterValue`'s own comparison methods (which have bespoke
        // `lt`/`le`/`gt`/`ge` semantics) rather than `Cow`'s defaults,
        // which would derive them from `partial_cmp` instead.
        let left = left.as_ref();
        let right = right.as_ref();
        match operator {
            BinaryOperator::Equals => wrap_bool(left == right),
            BinaryOperator::NotEquals => wrap_bool(left != right),
            BinaryOperator::Contains => wrap_bool(left.contains(right)),
            BinaryOperator::ContainsCs => wrap_bool(left.contains_cs(right)),
            BinaryOperator::In => wrap_bool(right.contains(left)),
            BinaryOperator::InCs => wrap_bool(right.contains_cs(left)),
            BinaryOperator::StartsWith => wrap_bool(left.startswith(right)),
            BinaryOperator::StartsWithCs => wrap_bool(left.startswith_cs(right)),
            BinaryOperator::EndsWith => wrap_bool(left.endswith(right)),
            BinaryOperator::EndsWithCs => wrap_bool(left.endswith_cs(right)),
            BinaryOperator::GreaterThan => wrap_bool(left.gt(right)),
            BinaryOperator::SmallerThan => wrap_bool(left.lt(right)),
            BinaryOperator::GreaterEqual => wrap_bool(left.ge(right)),
            BinaryOperator::SmallerEqual => wrap_bool(left.le(right)),
            BinaryOperator::Plus => add(left.to_owned(), right.to_owned()),
            BinaryOperator::Minus => subtract(left.to_owned(), right.to_owned()),
        }
    }

    fn visit_logical(
        &mut self,
        left: &'e Expr<'e>,
        operator: LogicalOperator,
        right: &'e Expr<'e>,
    ) -> Cow<'e, FilterValue<'e>> {
        let left = self.visit_expr(left);

        match operator {
            LogicalOperator::And if left.is_truthy() => self.visit_expr(right),
            LogicalOperator::And => left,
            LogicalOperator::Or if !left.is_truthy() => self.visit_expr(right),
            LogicalOperator::Or => left,
        }
    }

    fn visit_unary(
        &mut self,
        operator: UnaryOperator,
        right: &'e Expr<'e>,
    ) -> Cow<'e, FilterValue<'e>> {
        let right = self.visit_expr(right);

        match operator {
            UnaryOperator::Not => Cow::Owned(FilterValue::Bool(!right.is_truthy())),
        }
    }

    fn visit_like(&mut self, left: &'e Expr<'e>, glob: &'e Glob) -> Cow<'e, FilterValue<'e>> {
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
    ) -> Cow<'e, FilterValue<'e>> {
        let left = self.visit_expr(left);
        Cow::Owned(FilterValue::Bool(match_string_value(left.as_ref(), |s| {
            regex.is_match(s)
        })))
    }
}

/// Evaluates the `+` operator, returning [`FilterValue::Null`] for operand
/// combinations which cannot be meaningfully added together.
fn add<'a>(left: FilterValue<'a>, right: FilterValue<'a>) -> Cow<'a, FilterValue<'a>> {
    match (left, right) {
        (FilterValue::Number(a), FilterValue::Number(b)) => Cow::Owned(FilterValue::Number(a + b)),
        #[cfg(feature = "chrono")]
        (FilterValue::DateTime(a), FilterValue::Duration(b)) => Cow::Owned(
            a.checked_add_signed(b)
                .map(FilterValue::DateTime)
                .unwrap_or(FilterValue::Null),
        ),
        #[cfg(feature = "chrono")]
        (FilterValue::Duration(a), FilterValue::DateTime(b)) => Cow::Owned(
            b.checked_add_signed(a)
                .map(FilterValue::DateTime)
                .unwrap_or(FilterValue::Null),
        ),
        #[cfg(feature = "chrono")]
        (FilterValue::Duration(a), FilterValue::Duration(b)) => Cow::Owned(
            a.checked_add(&b)
                .map(FilterValue::Duration)
                .unwrap_or(FilterValue::Null),
        ),
        _ => Cow::Owned(FilterValue::Null),
    }
}

/// Evaluates the `-` operator, returning [`FilterValue::Null`] for operand
/// combinations which cannot be meaningfully subtracted from one another.
fn subtract<'a>(left: FilterValue<'a>, right: FilterValue<'a>) -> Cow<'a, FilterValue<'a>> {
    match (left, right) {
        (FilterValue::Number(a), FilterValue::Number(b)) => Cow::Owned(FilterValue::Number(a - b)),
        #[cfg(feature = "chrono")]
        (FilterValue::DateTime(a), FilterValue::Duration(b)) => Cow::Owned(
            a.checked_sub_signed(b)
                .map(FilterValue::DateTime)
                .unwrap_or(FilterValue::Null),
        ),
        #[cfg(feature = "chrono")]
        (FilterValue::DateTime(a), FilterValue::DateTime(b)) => {
            Cow::Owned(FilterValue::Duration(a.signed_duration_since(b)))
        }
        #[cfg(feature = "chrono")]
        (FilterValue::Duration(a), FilterValue::Duration(b)) => Cow::Owned(
            a.checked_sub(&b)
                .map(FilterValue::Duration)
                .unwrap_or(FilterValue::Null),
        ),
        _ => Cow::Owned(FilterValue::Null),
    }
}

/// Evaluates the `trim(string)` function, removing leading and trailing
/// whitespace from a string value. A secret string is trimmed too, but is
/// re-wrapped as a secret so that it stays redacted. Any other (non-string)
/// argument yields [`FilterValue::Null`], consistent with the language's
/// lenient handling of type mismatches.
fn trim<'a>(value: Cow<'a, FilterValue<'a>>) -> Cow<'a, FilterValue<'a>> {
    // Only string-like values can be trimmed; everything else yields null.
    // Checking before we take ownership means a non-string argument (such as a
    // tuple) is never cloned just to be dropped.
    let trimmable = matches!(value.as_ref(), FilterValue::String(_));
    #[cfg(feature = "secrecy")]
    let trimmable = trimmable || matches!(value.as_ref(), FilterValue::Secret(_));
    if !trimmable {
        return Cow::Owned(FilterValue::Null);
    }

    match value.into_owned() {
        // A borrowed string trims to a sub-slice of the very same bytes, so the
        // result stays borrowed and the trim itself allocates nothing.
        FilterValue::String(Cow::Borrowed(s)) => {
            Cow::Owned(FilterValue::String(Cow::Borrowed(s.trim())))
        }
        // An owned string already paid for its allocation when it was produced,
        // so trim it in place rather than allocating a fresh copy.
        FilterValue::String(Cow::Owned(mut s)) => {
            let end = s.trim_end().len();
            s.truncate(end);
            let start = s.len() - s.trim_start().len();
            s.drain(..start);
            Cow::Owned(FilterValue::String(Cow::Owned(s)))
        }
        // A secret is trimmed but stays wrapped so it remains redacted; the
        // trimmed text must be owned by a fresh secret, so this allocates.
        #[cfg(feature = "secrecy")]
        FilterValue::Secret(s) => Cow::Owned(FilterValue::secret(s.expose_secret().trim())),
        // Unreachable: the guard above accepts only string-like values.
        _ => Cow::Owned(FilterValue::Null),
    }
}

#[inline]
fn wrap_bool<'a>(value: bool) -> Cow<'a, FilterValue<'a>> {
    Cow::Owned(FilterValue::Bool(value))
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
        fn get(&self, property: &str) -> FilterValue<'_> {
            match property {
                "boolean" => true.into(),
                "string" => "Alice".into(),
                "padded" => "  Alice  ".into(),
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
    #[case("1 + 2 == 3", true)]
    #[case("1 + 2 + 3 == 6", true)] // chaining is left-associative
    #[case("5 - 2 == 3", true)]
    #[case("5 - 2 - 1 == 2", true)]
    #[case("5 - 2 + 1 == 4", true)]
    #[case("0 - 5 < 0", true)] // negative values via subtraction
    #[case("number + 1 == 2", true)]
    #[case("number + 1 > 1", true)] // arithmetic binds tighter than comparisons
    #[case("1 + null == null", true)] // mismatched operands evaluate to null
    #[case("\"a\" + \"b\" == null", true)] // there is no string concatenation
    #[case("true + true == null", true)]
    #[case("tuple + tuple == null", true)]
    fn arithmetic(#[case] filter: &str, #[case] expected: bool) {
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

    #[rstest]
    // Leading and trailing whitespace (of every kind) is removed...
    #[case("trim(\"  hello  \") == \"hello\"", true)]
    #[case("trim(\"\nhello\t\") == \"hello\"", true)]
    #[case("trim(\"hello\") == \"hello\"", true)]
    // ...while whitespace in the interior is preserved.
    #[case("trim(\"  a b  \") == \"a b\"", true)]
    // Empty and all-whitespace strings trim to the empty string.
    #[case("trim(\"\") == \"\"", true)]
    #[case("trim(\"   \") == \"\"", true)]
    // String properties are trimmed just like literals.
    #[case("trim(padded) == \"Alice\"", true)]
    #[case("trim(padded) == string", true)]
    // The trimmed result still compares case-insensitively.
    #[case("trim(padded) == \"alice\"", true)]
    // Non-string arguments yield null (the lenient type-mismatch behaviour).
    #[case("trim(number) == null", true)]
    #[case("trim(null) == null", true)]
    #[case("trim(boolean) == null", true)]
    #[case("trim(tuple) == null", true)]
    #[case("trim(names) == null", true)]
    fn trim_function(#[case] filter: &str, #[case] expected: bool) {
        assert_eq!(TestFilterable::matches(filter), expected);
    }

    #[cfg(feature = "secrecy")]
    #[test]
    fn trim_keeps_secrets_wrapped_and_redacted() {
        // A secret is trimmed just like a string, but the result stays a secret
        // so it can never leak through formatting.
        let trimmed = trim(Cow::Owned(FilterValue::secret("  hunter2  ")));
        let trimmed = trimmed.as_ref();

        assert!(matches!(trimmed, FilterValue::Secret(_)));
        assert_eq!(trimmed.to_string(), "[REDACTED]");
        // The trimmed secret compares equal to the trimmed plaintext.
        assert_eq!(trimmed, &FilterValue::String("hunter2".into()));
    }

    #[cfg(feature = "chrono")]
    mod chrono_tests {
        use super::*;

        #[rstest]
        // now() is evaluated at filtering time and produces a datetime.
        #[case("now()", true)]
        #[case("now() == null", false)]
        // Datetime arithmetic and comparisons.
        #[case("now() + 1h > now()", true)]
        #[case("now() - 1h < now()", true)]
        #[case("now() - 1h < now() + 1h", true)]
        #[case("now() - now() < 1s", true)]
        #[case("now() + 5m - 5m <= now()", true)]
        // Duration literals, comparisons, and arithmetic.
        #[case("5m < 1h", true)]
        #[case("1h > 90s", true)]
        #[case("60s == 1m", true)]
        #[case("1h30m == 90m", true)]
        #[case("30m + 30m == 1h", true)]
        #[case("1h - 30m == 30m", true)]
        // Durations are truthy iff they are non-zero.
        #[case("5m", true)]
        #[case("0s", false)]
        #[case("!0s", true)]
        // Mismatched operand types evaluate to null (and never match).
        #[case("5m == 300", false)]
        #[case("now() > 5", false)]
        #[case("5m + 5 == null", true)]
        #[case("now() + now() == null", true)]
        #[case("now() - 5 == null", true)]
        #[case("5m - now() == null", true)]
        fn datetime_filters(#[case] filter: &str, #[case] expected: bool) {
            assert_eq!(TestFilterable::matches(filter), expected);
        }
    }
}
