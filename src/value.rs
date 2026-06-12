use std::cmp::Ordering;
use std::fmt::{Debug, Display};

/// A trait for types which can be filtered by the filter system.
///
/// Types which implement this trait can be filtered through the use
/// of filter DSL expressions. A filter expression might look something
/// like the following:
///
/// ```text
/// repo.public && !repo.fork && repo.name in ["git-tool", "grey"]
/// ```
///
/// In this case, the [`Filter`](crate::Filter) would call [`Filterable::get`]
/// with the property keys it intends to retrieve, in this case: `repo.public`,
/// `repo.fork`, and `repo.name`. The [`Filterable`] implementation would
/// then return the appropriate [`FilterValue`] for each key.
///
/// ```
/// use filters::{FilterValue, Filterable};
///
/// struct Repo {
///     name: String,
///     public: bool,
///     fork: bool,
/// }
///
/// impl Filterable for Repo {
///     fn get(&self, key: &str) -> FilterValue {
///         match key {
///             "repo.name" => self.name.as_str().into(),
///             "repo.public" => self.public.into(),
///             "repo.fork" => self.fork.into(),
///             _ => FilterValue::Null,
///         }
///     }
/// }
/// ```
pub trait Filterable {
    /// Retrieve the value of a property key.
    ///
    /// This method should return the value of the property key as it
    /// pertains to the filterable object. If the key is not present,
    /// the method should return a [`FilterValue::Null`] value.
    fn get(&self, key: &str) -> FilterValue;
}

/// A value which may appear within a filter expression, either as a literal
/// or as the result of resolving a property on a [`Filterable`] object.
///
/// `FilterValue` implements [`From`] for most primitive Rust types (booleans,
/// numbers, strings, [`Option`]s, and vectors of values), making it easy to
/// construct from your own data within a [`Filterable::get`] implementation.
///
/// ```
/// use filters::FilterValue;
///
/// let value: FilterValue = 42.into();
/// assert_eq!(value, FilterValue::Number(42.0));
///
/// let value: FilterValue = Some("hello").into();
/// assert_eq!(value, FilterValue::String("hello".to_string()));
///
/// let value: FilterValue = None::<bool>.into();
/// assert_eq!(value, FilterValue::Null);
/// ```
///
/// Note that string equality comparisons between `FilterValue`s are
/// case-insensitive, mirroring the behaviour of the filter language itself.
///
/// ```
/// use filters::FilterValue;
///
/// let a: FilterValue = "Hello".into();
/// let b: FilterValue = "hello".into();
/// assert_eq!(a, b);
/// ```
#[derive(Clone, Default)]
pub enum FilterValue {
    /// The absence of a value, also returned for unknown property keys.
    #[default]
    Null,
    /// A boolean value (`true` or `false`).
    Bool(bool),
    /// A numeric value; all numbers are represented as 64-bit floats.
    Number(f64),
    /// A string value, compared case-insensitively by the filter language.
    String(String),
    /// An ordered list of values, written as `[a, b, c]` in filter expressions.
    Tuple(Vec<FilterValue>),
}

impl FilterValue {
    /// Determines whether this value is considered "truthy" by the filter language.
    ///
    /// Filters match an object when their expression evaluates to a truthy
    /// value. [`FilterValue::Null`], `false`, `0`, empty strings, and empty
    /// tuples are falsy; everything else is truthy.
    ///
    /// ```
    /// use filters::FilterValue;
    ///
    /// assert!(FilterValue::Bool(true).is_truthy());
    /// assert!(FilterValue::String("hello".to_string()).is_truthy());
    /// assert!(!FilterValue::Null.is_truthy());
    /// assert!(!FilterValue::Number(0.0).is_truthy());
    /// ```
    pub fn is_truthy(&self) -> bool {
        match self {
            FilterValue::Null => false,
            FilterValue::Bool(b) => *b,
            FilterValue::Number(n) => *n != 0.0,
            FilterValue::String(s) => !s.is_empty(),
            FilterValue::Tuple(v) => !v.is_empty(),
        }
    }

    /// Determines whether this value contains the provided value.
    ///
    /// For tuples, this checks whether any element is equal to `other`; for
    /// strings, it performs a case-insensitive substring search. All other
    /// combinations return `false`. This powers the `contains` and `in`
    /// operators in the filter language.
    ///
    /// ```
    /// use filters::FilterValue;
    ///
    /// let haystack: FilterValue = "Hello World".into();
    /// assert!(haystack.contains(&"world".into()));
    ///
    /// let tuple = FilterValue::Tuple(vec!["a".into(), "b".into()]);
    /// assert!(tuple.contains(&"a".into()));
    /// assert!(!tuple.contains(&"c".into()));
    /// ```
    pub fn contains(&self, other: &FilterValue) -> bool {
        match (self, other) {
            (FilterValue::Tuple(a), b) => a.iter().any(|ai| ai == b),
            (FilterValue::String(a), FilterValue::String(b)) => {
                a.to_lowercase().contains(&b.to_lowercase())
            }
            _ => false,
        }
    }

    /// Determines whether this value starts with the provided value.
    ///
    /// For strings, this performs a case-insensitive prefix test; for tuples,
    /// it checks whether any element is equal to `other`. This powers the
    /// `startswith` operator in the filter language.
    ///
    /// ```
    /// use filters::FilterValue;
    ///
    /// let value: FilterValue = "Hello World".into();
    /// assert!(value.startswith(&"hello".into()));
    /// assert!(!value.startswith(&"world".into()));
    /// ```
    pub fn startswith(&self, other: &FilterValue) -> bool {
        match (self, other) {
            (FilterValue::Tuple(a), b) => a.iter().any(|ai| ai == b),
            (FilterValue::String(a), FilterValue::String(b)) => {
                a.to_lowercase().starts_with(&b.to_lowercase())
            }
            _ => false,
        }
    }

    /// Determines whether this value ends with the provided value.
    ///
    /// For strings, this performs a case-insensitive suffix test; for tuples,
    /// it checks whether any element is equal to `other`. This powers the
    /// `endswith` operator in the filter language.
    ///
    /// ```
    /// use filters::FilterValue;
    ///
    /// let value: FilterValue = "Hello World".into();
    /// assert!(value.endswith(&"WORLD".into()));
    /// assert!(!value.endswith(&"hello".into()));
    /// ```
    pub fn endswith(&self, other: &FilterValue) -> bool {
        match (self, other) {
            (FilterValue::Tuple(a), b) => a.iter().any(|ai| ai == b),
            (FilterValue::String(a), FilterValue::String(b)) => {
                a.to_lowercase().ends_with(&b.to_lowercase())
            }
            _ => false,
        }
    }
}

impl PartialEq for FilterValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FilterValue::Null, FilterValue::Null) => true,
            (FilterValue::Bool(a), FilterValue::Bool(b)) => a == b,
            (FilterValue::Number(a), FilterValue::Number(b)) => a == b,
            (FilterValue::String(a), FilterValue::String(b)) => a.eq_ignore_ascii_case(b),
            (FilterValue::Tuple(a), FilterValue::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| a == b)
            }
            _ => false,
        }
    }
}

impl PartialOrd for FilterValue {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (FilterValue::Null, FilterValue::Null) => Some(Ordering::Equal),
            (FilterValue::Bool(a), FilterValue::Bool(b)) => a.partial_cmp(b),
            (FilterValue::Number(a), FilterValue::Number(b)) => a.partial_cmp(b),
            (FilterValue::String(a), FilterValue::String(b)) => a.partial_cmp(b),
            (FilterValue::Tuple(a), FilterValue::Tuple(b)) => {
                if a.len() != b.len() {
                    a.len().partial_cmp(&b.len())
                } else {
                    a.iter()
                        .zip(b.iter())
                        .map(|(x, y)| x.partial_cmp(y))
                        .find(|&cmp| cmp != Some(Ordering::Equal))
                        .unwrap_or(Some(Ordering::Equal))
                }
            }
            _ => None, // Return None for non-comparable types
        }
    }

    fn lt(&self, other: &Self) -> bool {
        match (self, other) {
            (FilterValue::Null, FilterValue::Null) => true,
            (FilterValue::Bool(a), FilterValue::Bool(b)) => a < b,
            (FilterValue::Number(a), FilterValue::Number(b)) => a < b,
            (FilterValue::String(a), FilterValue::String(b)) => a < b,
            (FilterValue::Tuple(a), FilterValue::Tuple(b)) => {
                a.len() <= b.len() && a.iter().zip(b.iter()).all(|(a, b)| a < b)
            }
            _ => false,
        }
    }

    fn le(&self, other: &Self) -> bool {
        match (self, other) {
            (FilterValue::Null, FilterValue::Null) => true,
            (FilterValue::Bool(a), FilterValue::Bool(b)) => a <= b,
            (FilterValue::Number(a), FilterValue::Number(b)) => a <= b,
            (FilterValue::String(a), FilterValue::String(b)) => a <= b,
            (FilterValue::Tuple(a), FilterValue::Tuple(b)) => {
                a.len() <= b.len() && a.iter().zip(b.iter()).all(|(a, b)| a <= b)
            }
            _ => false,
        }
    }

    fn gt(&self, other: &Self) -> bool {
        match (self, other) {
            (FilterValue::Null, FilterValue::Null) => true,
            (FilterValue::Bool(a), FilterValue::Bool(b)) => a > b,
            (FilterValue::Number(a), FilterValue::Number(b)) => a > b,
            (FilterValue::String(a), FilterValue::String(b)) => a > b,
            (FilterValue::Tuple(a), FilterValue::Tuple(b)) => {
                a.len() >= b.len() && a.iter().zip(b.iter()).all(|(a, b)| a > b)
            }
            _ => false,
        }
    }

    fn ge(&self, other: &Self) -> bool {
        match (self, other) {
            (FilterValue::Null, FilterValue::Null) => true,
            (FilterValue::Bool(a), FilterValue::Bool(b)) => a >= b,
            (FilterValue::Number(a), FilterValue::Number(b)) => a >= b,
            (FilterValue::String(a), FilterValue::String(b)) => a >= b,
            (FilterValue::Tuple(a), FilterValue::Tuple(b)) => {
                a.len() >= b.len() && a.iter().zip(b.iter()).all(|(a, b)| a >= b)
            }
            _ => false,
        }
    }
}

impl Display for FilterValue {
    /// Formats the value as it would appear within a filter expression.
    ///
    /// ```
    /// use filters::FilterValue;
    ///
    /// let value = FilterValue::Tuple(vec!["a".into(), 1.into(), FilterValue::Null]);
    /// assert_eq!(value.to_string(), r#"["a", 1, null]"#);
    /// ```
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FilterValue::Null => write!(f, "null"),
            FilterValue::Bool(b) => write!(f, "{}", b),
            FilterValue::Number(n) => write!(f, "{}", n),
            FilterValue::String(s) => {
                write!(f, "\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
            }
            FilterValue::Tuple(v) => {
                write!(f, "[")?;
                for (i, value) in v.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", value)?;
                }
                write!(f, "]")
            }
        }
    }
}

impl Debug for FilterValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self)
    }
}

impl From<bool> for FilterValue {
    fn from(b: bool) -> Self {
        FilterValue::Bool(b)
    }
}

macro_rules! number {
    ($t:ty) => {
        impl From<$t> for FilterValue {
            fn from(n: $t) -> Self {
                FilterValue::Number(n as f64)
            }
        }
    };
}

number!(i8);
number!(u8);
number!(i16);
number!(u16);
number!(f32);
number!(i32);
number!(u32);
number!(f64);
number!(i64);
number!(u64);

impl From<&str> for FilterValue {
    fn from(s: &str) -> Self {
        FilterValue::String(s.to_string())
    }
}

impl From<String> for FilterValue {
    fn from(s: String) -> Self {
        FilterValue::String(s)
    }
}

impl<T> From<Option<T>> for FilterValue
where
    T: Into<FilterValue>,
{
    fn from(o: Option<T>) -> Self {
        o.map_or(FilterValue::Null, Into::into)
    }
}

impl From<Vec<FilterValue>> for FilterValue {
    fn from(v: Vec<FilterValue>) -> Self {
        FilterValue::Tuple(v)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(FilterValue::Null, false)]
    #[case(FilterValue::Bool(false), false)]
    #[case(FilterValue::Bool(true), true)]
    #[case(FilterValue::Number(0.0), false)]
    #[case(FilterValue::Number(1.0), true)]
    #[case(FilterValue::String("".to_string()), false)]
    #[case(FilterValue::String("hello".to_string()), true)]
    #[case(FilterValue::Tuple(vec![]), false)]
    #[case(FilterValue::Tuple(vec![FilterValue::Bool(true)]), true)]
    fn test_truthy<V: Into<FilterValue>>(#[case] value: V, #[case] truthy: bool) {
        assert_eq!(value.into().is_truthy(), truthy);
    }

    #[test]
    fn test_bool_comparison() {
        assert!(FilterValue::Bool(false) < FilterValue::Bool(true));
        assert!(FilterValue::Bool(true) > FilterValue::Bool(false));
        assert_eq!(FilterValue::Bool(true), FilterValue::Bool(true));
        assert_eq!(FilterValue::Bool(false), FilterValue::Bool(false));
    }

    #[test]
    fn test_number_comparison() {
        assert!(FilterValue::Number(1.0) < FilterValue::Number(2.0));
        assert!(FilterValue::Number(2.0) > FilterValue::Number(1.0));
        assert_eq!(FilterValue::Number(2.0), FilterValue::Number(2.0));
    }

    #[test]
    fn test_string_comparison() {
        assert!(
            FilterValue::String(String::from("abc")) < FilterValue::String(String::from("xyz"))
        );
        assert!(
            FilterValue::String(String::from("xyz")) > FilterValue::String(String::from("abc"))
        );
        assert_eq!(
            FilterValue::String(String::from("abc")),
            FilterValue::String(String::from("abc"))
        );
    }

    #[test]
    fn test_string_equality_is_case_insensitive() {
        assert_eq!(
            FilterValue::String(String::from("Hello World")),
            FilterValue::String(String::from("hello world"))
        );
        assert_ne!(
            FilterValue::String(String::from("Hello World")),
            FilterValue::String(String::from("goodbye world"))
        );
    }

    #[test]
    fn test_tuple_comparison() {
        // The `<` and `>` operators require every paired element to compare
        // accordingly, while ordering between different-length tuples is
        // driven by their lengths.
        assert!(
            FilterValue::Tuple(vec![1.into(), 2.into()])
                < FilterValue::Tuple(vec![3.into(), 4.into()])
        );
        assert!(
            FilterValue::Tuple(vec![3.into(), 4.into()])
                > FilterValue::Tuple(vec![1.into(), 2.into()])
        );

        let short = FilterValue::Tuple(vec![1.into()]);
        let long = FilterValue::Tuple(vec![1.into(), 2.into()]);
        assert_eq!(short.partial_cmp(&long), Some(Ordering::Less));
        assert_eq!(long.partial_cmp(&short), Some(Ordering::Greater));

        assert_eq!(
            FilterValue::Tuple(vec![1.into(), 2.into()]),
            FilterValue::Tuple(vec![1.into(), 2.into()])
        );
        assert_ne!(
            FilterValue::Tuple(vec![1.into(), 2.into()]),
            FilterValue::Tuple(vec![2.into(), 1.into()])
        );
    }

    #[rstest]
    #[case(FilterValue::Null, FilterValue::Bool(true))]
    #[case(FilterValue::Bool(true), FilterValue::Number(1.0))]
    #[case(FilterValue::Number(1.0), FilterValue::String("1".to_string()))]
    #[case(FilterValue::String("a".to_string()), FilterValue::Tuple(vec!["a".into()]))]
    fn test_mismatched_types_are_not_equal_or_ordered(
        #[case] left: FilterValue,
        #[case] right: FilterValue,
    ) {
        assert_ne!(left, right);
        assert_eq!(left.partial_cmp(&right), None);
        assert!(!left.lt(&right));
        assert!(!left.le(&right));
        assert!(!left.gt(&right));
        assert!(!left.ge(&right));
    }

    #[rstest]
    #[case(true.into(), FilterValue::Bool(true))]
    #[case(42i8.into(), FilterValue::Number(42.0))]
    #[case(42u8.into(), FilterValue::Number(42.0))]
    #[case(42i16.into(), FilterValue::Number(42.0))]
    #[case(42u16.into(), FilterValue::Number(42.0))]
    #[case(42i32.into(), FilterValue::Number(42.0))]
    #[case(42u32.into(), FilterValue::Number(42.0))]
    #[case(42i64.into(), FilterValue::Number(42.0))]
    #[case(42u64.into(), FilterValue::Number(42.0))]
    #[case(4.2f32.into(), FilterValue::Number(4.2f32 as f64))]
    #[case(4.2f64.into(), FilterValue::Number(4.2))]
    #[case("hello".into(), FilterValue::String("hello".to_string()))]
    #[case(String::from("hello").into(), FilterValue::String("hello".to_string()))]
    #[case(Some(1).into(), FilterValue::Number(1.0))]
    #[case(None::<i32>.into(), FilterValue::Null)]
    #[case(vec![FilterValue::Null].into(), FilterValue::Tuple(vec![FilterValue::Null]))]
    fn test_conversions(#[case] converted: FilterValue, #[case] expected: FilterValue) {
        assert_eq!(converted, expected);
    }

    #[rstest]
    #[case(FilterValue::Null, "null")]
    #[case(FilterValue::Bool(true), "true")]
    #[case(FilterValue::Bool(false), "false")]
    #[case(FilterValue::Number(1.5), "1.5")]
    #[case(FilterValue::String("hello".to_string()), "\"hello\"")]
    #[case(FilterValue::String("say \"hi\"".to_string()), "\"say \\\"hi\\\"\"")]
    #[case(FilterValue::String("back\\slash".to_string()), "\"back\\\\slash\"")]
    #[case(FilterValue::Tuple(vec![]), "[]")]
    #[case(FilterValue::Tuple(vec![1.into(), "a".into()]), "[1, \"a\"]")]
    fn test_display(#[case] value: FilterValue, #[case] expected: &str) {
        assert_eq!(value.to_string(), expected);
        assert_eq!(format!("{value:?}"), expected);
    }

    #[rstest]
    #[case("Hello World".into(), "world".into(), true)]
    #[case("Hello World".into(), "WORLD".into(), true)]
    #[case("Hello World".into(), "mars".into(), false)]
    #[case(FilterValue::Tuple(vec!["a".into(), "b".into()]), "A".into(), true)]
    #[case(FilterValue::Tuple(vec!["a".into(), "b".into()]), "c".into(), false)]
    #[case(FilterValue::Tuple(vec![]), FilterValue::Null, false)]
    #[case(FilterValue::Null, FilterValue::Null, false)]
    #[case(FilterValue::Number(12.0), FilterValue::Number(2.0), false)]
    fn test_contains(
        #[case] value: FilterValue,
        #[case] other: FilterValue,
        #[case] expected: bool,
    ) {
        assert_eq!(value.contains(&other), expected);
    }

    #[rstest]
    #[case("Hello World".into(), "hello".into(), true)]
    #[case("Hello World".into(), "world".into(), false)]
    #[case(FilterValue::Tuple(vec!["a".into()]), "a".into(), true)]
    #[case(FilterValue::Null, "a".into(), false)]
    #[case("Hello".into(), FilterValue::Null, false)]
    fn test_startswith(
        #[case] value: FilterValue,
        #[case] other: FilterValue,
        #[case] expected: bool,
    ) {
        assert_eq!(value.startswith(&other), expected);
    }

    #[rstest]
    #[case("Hello World".into(), "world".into(), true)]
    #[case("Hello World".into(), "hello".into(), false)]
    #[case(FilterValue::Tuple(vec!["a".into()]), "a".into(), true)]
    #[case(FilterValue::Null, "a".into(), false)]
    #[case("Hello".into(), FilterValue::Null, false)]
    fn test_endswith(
        #[case] value: FilterValue,
        #[case] other: FilterValue,
        #[case] expected: bool,
    ) {
        assert_eq!(value.endswith(&other), expected);
    }

    #[test]
    fn test_default_is_null() {
        assert_eq!(FilterValue::default(), FilterValue::Null);
    }
}
