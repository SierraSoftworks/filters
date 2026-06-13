use std::cmp::Ordering;
use std::fmt::{Debug, Display};

use crate::case_sensitivity::{
    caseless_contains, caseless_ends_with, caseless_eq, caseless_starts_with,
};

#[cfg(feature = "secrecy")]
use secrecy::ExposeSecret;

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
/// Case is folded character-by-character using the language's Unicode
/// case-folding rules (with all Greek sigma forms treated as equivalent),
/// so multi-character folds such as `ß` → `ss` compare equal too.
///
/// ```
/// use filters::FilterValue;
///
/// let a: FilterValue = "Hello".into();
/// let b: FilterValue = "hello".into();
/// assert_eq!(a, b);
///
/// let a: FilterValue = "STRASSE".into();
/// let b: FilterValue = "straße".into();
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
    /// A secret string value which behaves exactly like a [`FilterValue::String`]
    /// in comparisons, but is always redacted as `[REDACTED]` when formatted.
    #[cfg(feature = "secrecy")]
    Secret(secrecy::SecretString),
}

impl FilterValue {
    /// Creates a secret string value backed by a [`secrecy::SecretString`].
    ///
    /// Secret values behave exactly like a [`FilterValue::String`] in every
    /// comparison operation (equality, ordering, `contains`, `in`,
    /// `startswith`, `endswith`, and truthiness), but are always redacted as
    /// `[REDACTED]` when formatted with [`Display`] or [`Debug`], making it
    /// impossible to leak the underlying secret through logging.
    ///
    /// Note that, like every comparison in this crate, secret comparisons are
    /// not constant-time and should not be relied upon to defend against
    /// timing attacks.
    ///
    /// ```
    /// use filters::FilterValue;
    ///
    /// let password = FilterValue::secret("hunter2");
    ///
    /// // Secrets compare exactly like strings (case-insensitively for equality)...
    /// assert_eq!(password, FilterValue::String("HUNTER2".to_string()));
    /// assert!(password.contains(&"unter".into()));
    ///
    /// // ...but they are always redacted when formatted.
    /// assert_eq!(password.to_string(), "[REDACTED]");
    /// assert_eq!(format!("{password:?}"), "[REDACTED]");
    /// ```
    #[cfg(feature = "secrecy")]
    pub fn secret(value: impl Into<String>) -> Self {
        FilterValue::Secret(secrecy::SecretString::from(value.into()))
    }

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
            #[cfg(feature = "secrecy")]
            FilterValue::Secret(s) => !s.expose_secret().is_empty(),
        }
    }

    /// Determines whether this value contains the provided value.
    ///
    /// For tuples, this checks whether any element is equal to `other`; for
    /// strings, it performs a case-insensitive substring search. All other
    /// combinations return `false`. This powers the `contains` and `in`
    /// operators in the filter language.
    ///
    /// The string comparison case-folds both operands character-by-character
    /// without allocating, using the same Unicode case-folding rules as the
    /// rest of the filter language: all Greek sigma forms (`Σ`, `σ`, and the
    /// final-position `ς`) are treated as equivalent regardless of where they
    /// appear in a word, and multi-character folds such as `ß` → `ss`
    /// participate fully.
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
            (FilterValue::String(a), FilterValue::String(b)) => caseless_contains(a, b),
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => {
                caseless_contains(a.expose_secret(), b.expose_secret())
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => {
                caseless_contains(a.expose_secret(), b)
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => {
                caseless_contains(a, b.expose_secret())
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
    /// The string comparison case-folds both operands character-by-character
    /// without allocating, using the same Unicode case-folding rules as the
    /// rest of the filter language (see [`FilterValue::contains`]).
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
            (FilterValue::String(a), FilterValue::String(b)) => caseless_starts_with(a, b),
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => {
                caseless_starts_with(a.expose_secret(), b.expose_secret())
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => {
                caseless_starts_with(a.expose_secret(), b)
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => {
                caseless_starts_with(a, b.expose_secret())
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
    /// The string comparison case-folds both operands character-by-character
    /// without allocating, using the same Unicode case-folding rules as the
    /// rest of the filter language (see [`FilterValue::contains`]).
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
            (FilterValue::String(a), FilterValue::String(b)) => caseless_ends_with(a, b),
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => {
                caseless_ends_with(a.expose_secret(), b.expose_secret())
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => {
                caseless_ends_with(a.expose_secret(), b)
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => {
                caseless_ends_with(a, b.expose_secret())
            }
            _ => false,
        }
    }

    /// Determines whether this value is equal to the provided value, comparing
    /// strings case-*sensitively*.
    ///
    /// This is the case-sensitive counterpart of the `==` operator (and the
    /// [`PartialEq`] implementation): tuples compare their elements with
    /// `eq_cs` recursively, and all other variants behave exactly as `==`
    /// does. It underpins tuple membership for the `contains_cs` and `in_cs`
    /// operators in the filter language.
    ///
    /// ```
    /// use filters::FilterValue;
    ///
    /// let value: FilterValue = "Hello".into();
    /// assert!(value.eq_cs(&"Hello".into()));
    /// assert!(!value.eq_cs(&"hello".into()));
    /// ```
    pub fn eq_cs(&self, other: &FilterValue) -> bool {
        match (self, other) {
            (FilterValue::String(a), FilterValue::String(b)) => a == b,
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => a.expose_secret() == b.expose_secret(),
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => a.expose_secret() == b,
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => a == b.expose_secret(),
            (FilterValue::Tuple(a), FilterValue::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| a.eq_cs(b))
            }
            _ => self == other,
        }
    }

    /// Determines whether this value contains the provided value, comparing
    /// strings case-*sensitively*.
    ///
    /// This is the case-sensitive counterpart of [`FilterValue::contains`]:
    /// tuples check whether any element is [`eq_cs`](FilterValue::eq_cs) to
    /// `other`, strings perform an exact substring search, and all other
    /// combinations return `false`. This powers the `contains_cs` and `in_cs`
    /// operators in the filter language.
    ///
    /// ```
    /// use filters::FilterValue;
    ///
    /// let haystack: FilterValue = "Hello World".into();
    /// assert!(haystack.contains_cs(&"World".into()));
    /// assert!(!haystack.contains_cs(&"world".into()));
    /// ```
    pub fn contains_cs(&self, other: &FilterValue) -> bool {
        match (self, other) {
            (FilterValue::Tuple(a), b) => a.iter().any(|ai| ai.eq_cs(b)),
            (FilterValue::String(a), FilterValue::String(b)) => a.contains(b.as_str()),
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => a.expose_secret().contains(b.expose_secret()),
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => a.expose_secret().contains(b),
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => a.contains(b.expose_secret()),
            _ => false,
        }
    }

    /// Determines whether this value starts with the provided value, comparing
    /// strings case-*sensitively*.
    ///
    /// This is the case-sensitive counterpart of [`FilterValue::startswith`],
    /// powering the `startswith_cs` operator in the filter language.
    ///
    /// ```
    /// use filters::FilterValue;
    ///
    /// let value: FilterValue = "Hello World".into();
    /// assert!(value.startswith_cs(&"Hello".into()));
    /// assert!(!value.startswith_cs(&"hello".into()));
    /// ```
    pub fn startswith_cs(&self, other: &FilterValue) -> bool {
        match (self, other) {
            (FilterValue::Tuple(a), b) => a.iter().any(|ai| ai.eq_cs(b)),
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => a.expose_secret().starts_with(b.expose_secret()),
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => a.expose_secret().starts_with(b),
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => a.starts_with(b.expose_secret()),
            (FilterValue::String(a), FilterValue::String(b)) => a.starts_with(b.as_str()),
            _ => false,
        }
    }

    /// Determines whether this value ends with the provided value, comparing
    /// strings case-*sensitively*.
    ///
    /// This is the case-sensitive counterpart of [`FilterValue::endswith`],
    /// powering the `endswith_cs` operator in the filter language.
    ///
    /// ```
    /// use filters::FilterValue;
    ///
    /// let value: FilterValue = "Hello World".into();
    /// assert!(value.endswith_cs(&"World".into()));
    /// assert!(!value.endswith_cs(&"WORLD".into()));
    /// ```
    pub fn endswith_cs(&self, other: &FilterValue) -> bool {
        match (self, other) {
            (FilterValue::Tuple(a), b) => a.iter().any(|ai| ai.eq_cs(b)),
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => a.expose_secret().ends_with(b.expose_secret()),
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => a.expose_secret().ends_with(b),
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => a.ends_with(b.expose_secret()),
            (FilterValue::String(a), FilterValue::String(b)) => a.ends_with(b.as_str()),
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
            (FilterValue::String(a), FilterValue::String(b)) => caseless_eq(a, b),
            (FilterValue::Tuple(a), FilterValue::Tuple(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| a == b)
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => {
                caseless_eq(a.expose_secret(), b.expose_secret())
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => {
                caseless_eq(a.expose_secret(), b)
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => {
                caseless_eq(a, b.expose_secret())
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
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => {
                a.expose_secret().partial_cmp(b.expose_secret())
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => {
                a.expose_secret().partial_cmp(b.as_str())
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => {
                a.as_str().partial_cmp(b.expose_secret())
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
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => {
                a.expose_secret() < b.expose_secret()
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => a.expose_secret() < b.as_str(),
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => a.as_str() < b.expose_secret(),
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
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => {
                a.expose_secret() <= b.expose_secret()
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => a.expose_secret() <= b.as_str(),
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => a.as_str() <= b.expose_secret(),
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
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => {
                a.expose_secret() > b.expose_secret()
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => a.expose_secret() > b.as_str(),
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => a.as_str() > b.expose_secret(),
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
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::Secret(b)) => {
                a.expose_secret() >= b.expose_secret()
            }
            #[cfg(feature = "secrecy")]
            (FilterValue::Secret(a), FilterValue::String(b)) => a.expose_secret() >= b.as_str(),
            #[cfg(feature = "secrecy")]
            (FilterValue::String(a), FilterValue::Secret(b)) => a.as_str() >= b.expose_secret(),
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
    ///
    /// Secret values (available with the `secrecy` feature) are always
    /// formatted as `[REDACTED]`, never as their underlying string.
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
            #[cfg(feature = "secrecy")]
            FilterValue::Secret(_) => write!(f, "[REDACTED]"),
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

#[cfg(feature = "secrecy")]
impl From<secrecy::SecretString> for FilterValue {
    /// Wraps a [`secrecy::SecretString`] as a [`FilterValue::Secret`].
    ///
    /// ```
    /// use filters::FilterValue;
    /// use secrecy::SecretString;
    ///
    /// let value: FilterValue = SecretString::from("hunter2").into();
    /// assert_eq!(value, FilterValue::String("hunter2".to_string()));
    /// assert_eq!(value.to_string(), "[REDACTED]");
    /// ```
    fn from(s: secrecy::SecretString) -> Self {
        FilterValue::Secret(s)
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

        // Equality folds case using the language's Unicode case-folding
        // rules, including non-ASCII characters and multi-character folds.
        assert_eq!(
            FilterValue::String(String::from("JÜRGEN")),
            FilterValue::String(String::from("jürgen"))
        );
        assert_eq!(
            FilterValue::String(String::from("ΛΟΓΟΣ")),
            FilterValue::String(String::from("λογος"))
        );
        assert_eq!(
            FilterValue::String(String::from("straße")),
            FilterValue::String(String::from("STRASSE"))
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

    #[rstest]
    #[case("Hello".into(), "Hello".into(), true)]
    #[case("Hello".into(), "hello".into(), false)]
    #[case("straße".into(), "STRASSE".into(), false)] // no case folding at all
    #[case(FilterValue::Null, FilterValue::Null, true)]
    #[case(FilterValue::Bool(true), FilterValue::Bool(true), true)]
    #[case(FilterValue::Number(1.0), FilterValue::Number(1.0), true)]
    #[case(FilterValue::Tuple(vec!["A".into()]), FilterValue::Tuple(vec!["A".into()]), true)]
    #[case(FilterValue::Tuple(vec!["A".into()]), FilterValue::Tuple(vec!["a".into()]), false)]
    #[case("1".into(), FilterValue::Number(1.0), false)]
    fn test_eq_cs(#[case] left: FilterValue, #[case] right: FilterValue, #[case] expected: bool) {
        assert_eq!(left.eq_cs(&right), expected);
        assert_eq!(right.eq_cs(&left), expected);
    }

    #[rstest]
    #[case("Hello World".into(), "World".into(), true)]
    #[case("Hello World".into(), "world".into(), false)]
    #[case(FilterValue::Tuple(vec!["a".into(), "B".into()]), "B".into(), true)]
    #[case(FilterValue::Tuple(vec!["a".into(), "B".into()]), "b".into(), false)]
    #[case(FilterValue::Null, FilterValue::Null, false)]
    #[case(FilterValue::Number(12.0), FilterValue::Number(2.0), false)]
    fn test_contains_cs(
        #[case] value: FilterValue,
        #[case] other: FilterValue,
        #[case] expected: bool,
    ) {
        assert_eq!(value.contains_cs(&other), expected);
    }

    #[rstest]
    #[case("Hello World".into(), "Hello".into(), true)]
    #[case("Hello World".into(), "hello".into(), false)]
    #[case(FilterValue::Tuple(vec!["A".into()]), "A".into(), true)]
    #[case(FilterValue::Tuple(vec!["A".into()]), "a".into(), false)]
    #[case(FilterValue::Null, "a".into(), false)]
    fn test_startswith_cs(
        #[case] value: FilterValue,
        #[case] other: FilterValue,
        #[case] expected: bool,
    ) {
        assert_eq!(value.startswith_cs(&other), expected);
    }

    #[rstest]
    #[case("Hello World".into(), "World".into(), true)]
    #[case("Hello World".into(), "WORLD".into(), false)]
    #[case(FilterValue::Tuple(vec!["A".into()]), "A".into(), true)]
    #[case(FilterValue::Tuple(vec!["A".into()]), "a".into(), false)]
    #[case(FilterValue::Null, "a".into(), false)]
    fn test_endswith_cs(
        #[case] value: FilterValue,
        #[case] other: FilterValue,
        #[case] expected: bool,
    ) {
        assert_eq!(value.endswith_cs(&other), expected);
    }

    /// The case-insensitive string operations treat all Greek sigma forms
    /// (`Σ`, `σ`, and the word-final `ς`) as equivalent, regardless of where
    /// they appear within a word.
    ///
    /// This intentionally diverges from [`str::to_lowercase`]'s context
    /// sensitive final-sigma rule (which would, for example, consider
    /// `"ΛΟΓΟΣ"` *not* to end with `"Σ"` because the haystack lowercases to
    /// `"λογος"` while the needle lowercases to `"σ"`). Folding every sigma
    /// to `σ` mirrors Unicode simple case folding and gives
    /// position-independent results.
    #[rstest]
    #[case("ΛΟΓΟΣ", "Σ")] // upper-case needle vs word-final position
    #[case("ΛΟΓΟΣ", "ς")] // final-sigma needle
    #[case("ΛΟΓΟΣ", "σ")] // regular-sigma needle
    #[case("λογος", "Σ")] // word-final sigma in the haystack
    fn test_greek_sigma_forms_are_equivalent(#[case] haystack: &str, #[case] needle: &str) {
        let haystack: FilterValue = haystack.into();
        let needle: FilterValue = needle.into();

        assert!(haystack.endswith(&needle));
        assert!(haystack.contains(&needle));
    }

    /// Characters whose case-folded form expands to multiple characters
    /// (such as `İ`, which folds to `i` followed by a combining dot above,
    /// or `ß`, which folds to `ss`) participate fully in the comparison.
    #[rstest]
    #[case("İstanbul", "i\u{307}stanbul", true)] // expanded folded form
    #[case("İstanbul", "\u{307}stanbul", true)] // matches mid-expansion, as str::contains would
    #[case("İstanbul", "istanbul", false)] // the combining mark is significant
    #[case("straße", "STRASSE", true)] // ß folds to ss
    #[case("groß", "ss", true)]
    #[case("gross", "ß", true)] // ...in the needle too
    fn test_multi_char_lowercase_expansions(
        #[case] haystack: &str,
        #[case] needle: &str,
        #[case] expected: bool,
    ) {
        let haystack: FilterValue = haystack.into();
        let needle: FilterValue = needle.into();

        assert_eq!(haystack.contains(&needle), expected);
    }

    #[cfg(feature = "secrecy")]
    mod secrecy_tests {
        use super::*;

        #[rstest]
        #[case(FilterValue::secret(""), false)]
        #[case(FilterValue::secret("hunter2"), true)]
        fn test_secret_truthy(#[case] value: FilterValue, #[case] truthy: bool) {
            assert_eq!(value.is_truthy(), truthy);
        }

        #[rstest]
        #[case(FilterValue::secret("hunter2"), FilterValue::secret("hunter2"), true)]
        #[case(FilterValue::secret("hunter2"), FilterValue::secret("HUNTER2"), true)]
        #[case(
            FilterValue::secret("hunter2"),
            FilterValue::secret("swordfish"),
            false
        )]
        #[case(FilterValue::secret("hunter2"), "hunter2".into(), true)]
        #[case(FilterValue::secret("hunter2"), "HUNTER2".into(), true)]
        #[case("HUNTER2".into(), FilterValue::secret("hunter2"), true)]
        #[case("swordfish".into(), FilterValue::secret("hunter2"), false)]
        fn test_secret_equality(
            #[case] left: FilterValue,
            #[case] right: FilterValue,
            #[case] equal: bool,
        ) {
            assert_eq!(left == right, equal);
            assert_eq!(left != right, !equal);
        }

        #[rstest]
        #[case(FilterValue::secret("abc"), FilterValue::secret("xyz"))]
        #[case(FilterValue::secret("abc"), "xyz".into())]
        #[case("abc".into(), FilterValue::secret("xyz"))]
        fn test_secret_ordering(#[case] smaller: FilterValue, #[case] larger: FilterValue) {
            assert_eq!(smaller.partial_cmp(&larger), Some(Ordering::Less));
            assert_eq!(larger.partial_cmp(&smaller), Some(Ordering::Greater));
            assert!(smaller < larger);
            assert!(smaller <= larger);
            assert!(larger > smaller);
            assert!(larger >= smaller);
            assert!(!smaller.gt(&larger));
            assert!(!smaller.ge(&larger));
            assert!(!larger.lt(&smaller));
            assert!(!larger.le(&smaller));
        }

        #[rstest]
        #[case(FilterValue::secret("Hello World"), "world".into(), true)]
        #[case(FilterValue::secret("Hello World"), "mars".into(), false)]
        #[case("Hello World".into(), FilterValue::secret("WORLD"), true)]
        #[case("Hello World".into(), FilterValue::secret("mars"), false)]
        #[case(FilterValue::secret("Hello World"), FilterValue::secret("WORLD"), true)]
        #[case(FilterValue::Tuple(vec![FilterValue::secret("a"), "b".into()]), "A".into(), true)]
        #[case(FilterValue::Tuple(vec!["a".into(), "b".into()]), FilterValue::secret("B"), true)]
        #[case(FilterValue::Tuple(vec!["a".into(), "b".into()]), FilterValue::secret("c"), false)]
        fn test_secret_contains(
            #[case] value: FilterValue,
            #[case] other: FilterValue,
            #[case] expected: bool,
        ) {
            assert_eq!(value.contains(&other), expected);
        }

        #[rstest]
        #[case(FilterValue::secret("Hello World"), "hello".into(), true)]
        #[case(FilterValue::secret("Hello World"), "world".into(), false)]
        #[case("Hello World".into(), FilterValue::secret("HELLO"), true)]
        #[case("Hello World".into(), FilterValue::secret("world"), false)]
        #[case(FilterValue::secret("Hello World"), FilterValue::secret("HELLO"), true)]
        fn test_secret_startswith(
            #[case] value: FilterValue,
            #[case] other: FilterValue,
            #[case] expected: bool,
        ) {
            assert_eq!(value.startswith(&other), expected);
        }

        #[rstest]
        #[case(FilterValue::secret("Hello World"), "WORLD".into(), true)]
        #[case(FilterValue::secret("Hello World"), "hello".into(), false)]
        #[case("Hello World".into(), FilterValue::secret("world"), true)]
        #[case("Hello World".into(), FilterValue::secret("hello"), false)]
        #[case(FilterValue::secret("Hello World"), FilterValue::secret("world"), true)]
        fn test_secret_endswith(
            #[case] value: FilterValue,
            #[case] other: FilterValue,
            #[case] expected: bool,
        ) {
            assert_eq!(value.endswith(&other), expected);
        }

        #[rstest]
        #[case(FilterValue::Null)]
        #[case(FilterValue::Bool(true))]
        #[case(FilterValue::Number(1.0))]
        #[case(FilterValue::Tuple(vec!["hunter2".into()]))]
        fn test_secrets_are_not_equal_or_ordered_against_other_types(#[case] other: FilterValue) {
            let secret = FilterValue::secret("hunter2");
            assert_ne!(secret, other);
            assert_ne!(other, secret);
            assert_eq!(secret.partial_cmp(&other), None);
            assert_eq!(other.partial_cmp(&secret), None);
            assert!(!secret.lt(&other));
            assert!(!secret.le(&other));
            assert!(!secret.gt(&other));
            assert!(!secret.ge(&other));
        }

        #[rstest]
        #[case(FilterValue::secret("hunter2"), "[REDACTED]")]
        #[case(FilterValue::secret(""), "[REDACTED]")]
        #[case(
            FilterValue::Tuple(vec!["a".into(), FilterValue::secret("hunter2"), 1.into()]),
            "[\"a\", [REDACTED], 1]"
        )]
        fn test_secret_display_is_redacted(#[case] value: FilterValue, #[case] expected: &str) {
            assert_eq!(value.to_string(), expected);
            assert_eq!(format!("{value:?}"), expected);
            assert!(!value.to_string().contains("hunter2"));
            assert!(!format!("{value:?}").contains("hunter2"));
        }

        #[test]
        fn test_secret_conversions() {
            let secret: FilterValue = secrecy::SecretString::from("hunter2").into();
            assert_eq!(secret, FilterValue::secret("hunter2"));
            assert!(matches!(secret, FilterValue::Secret(_)));
            assert!(matches!(
                FilterValue::secret(String::from("hunter2")),
                FilterValue::Secret(_)
            ));
        }

        /// For every comparison operation, a secret must behave exactly as the
        /// equivalent string would — whichever side of the operator it is on.
        #[rstest]
        #[case("hunter2", "hunter2")]
        #[case("hunter2", "HUNTER2")]
        #[case("hunter2", "swordfish")]
        #[case("abc", "abd")]
        #[case("abd", "abc")]
        #[case("Hello World", "WORLD")]
        #[case("Hello World", "hello")]
        #[case("", "")]
        #[case("", "a")]
        #[case("ÜBER", "über")]
        fn test_secrets_behave_exactly_like_strings(#[case] secret: &str, #[case] other: &str) {
            let as_secret = FilterValue::secret(secret);
            let as_string = FilterValue::String(secret.to_string());
            let other = FilterValue::String(other.to_string());

            assert_eq!(as_secret == other, as_string == other, "{secret} == {other}");
            assert_eq!(other == as_secret, other == as_string, "{other} == {secret}");
            assert_eq!(as_secret.partial_cmp(&other), as_string.partial_cmp(&other), "{secret} cmp {other}");
            assert_eq!(other.partial_cmp(&as_secret), other.partial_cmp(&as_string), "{other} cmp {secret}");
            assert_eq!(as_secret < other, as_string < other, "{secret} < {other}");
            assert_eq!(other < as_secret, other < as_string, "{other} < {secret}");
            assert_eq!(as_secret <= other, as_string <= other, "{secret} <= {other}");
            assert_eq!(other <= as_secret, other <= as_string, "{other} <= {secret}");
            assert_eq!(as_secret > other, as_string > other, "{secret} > {other}");
            assert_eq!(other > as_secret, other > as_string, "{other} > {secret}");
            assert_eq!(as_secret >= other, as_string >= other, "{secret} >= {other}");
            assert_eq!(other >= as_secret, other >= as_string, "{other} >= {secret}");
            assert_eq!(as_secret.contains(&other), as_string.contains(&other), "{secret} contains {other}");
            assert_eq!(other.contains(&as_secret), other.contains(&as_string), "{other} contains {secret}");
            assert_eq!(as_secret.startswith(&other), as_string.startswith(&other), "{secret} starts with {other}");
            assert_eq!(other.startswith(&as_secret), other.startswith(&as_string), "{other} starts with {secret}");
            assert_eq!(as_secret.endswith(&other), as_string.endswith(&other), "{secret} ends with {other}");
            assert_eq!(other.endswith(&as_secret), other.endswith(&as_string), "{other} ends with {secret}");
            assert_eq!(as_secret.is_truthy(), as_string.is_truthy(), "{secret} is_truthy");
        }
    }
}
