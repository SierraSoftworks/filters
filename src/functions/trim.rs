use std::borrow::Cow;

#[cfg(feature = "secrecy")]
use secrecy::ExposeSecret;

use crate::FilterValue;

use super::Function;

/// The `trim(string)` function: removes leading and trailing whitespace from a
/// string, leaving the interior untouched.
///
/// A borrowed string argument trims to a sub-slice of the very same bytes, so
/// the common case allocates nothing. A secret string is trimmed too, but is
/// re-wrapped as a secret so that it stays redacted. Any other (non-string)
/// argument yields [`FilterValue::Null`], consistent with the language's lenient
/// handling of type mismatches.
pub(crate) struct Trim;

impl Function for Trim {
    fn name(&self) -> &str {
        "trim"
    }

    fn arity(&self) -> usize {
        1
    }

    fn call<'a>(&self, args: &[Cow<'a, FilterValue<'a>>]) -> Cow<'a, FilterValue<'a>> {
        match args[0].as_ref() {
            // A borrowed string trims to a sub-slice of the very same bytes
            // (lifetime `'a`), so the result stays borrowed and allocates
            // nothing — the zero-allocation common case.
            FilterValue::String(Cow::Borrowed(s)) => {
                Cow::Owned(FilterValue::String(Cow::Borrowed(s.trim())))
            }
            // An owned string is borrowed from the (transient) argument, so the
            // trimmed slice must be copied into a fresh owned string.
            FilterValue::String(Cow::Owned(s)) => {
                Cow::Owned(FilterValue::String(Cow::Owned(s.trim().to_string())))
            }
            // A secret is trimmed but stays wrapped so it remains redacted.
            #[cfg(feature = "secrecy")]
            FilterValue::Secret(s) => Cow::Owned(FilterValue::secret(s.expose_secret().trim())),
            // Every other kind of value yields null.
            _ => Cow::Owned(FilterValue::Null),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::{FilterValue, functions::Function};

    use super::Trim;

    #[test]
    fn name_and_arity() {
        assert_eq!(Trim.name(), "trim");
        assert_eq!(Trim.arity(), 1);
    }

    #[test]
    fn borrowed_strings_trim_to_a_borrowed_sub_slice() {
        let result = Trim.call(&[Cow::Owned(FilterValue::String(Cow::Borrowed("  hi  ")))]);
        let FilterValue::String(cow) = result.as_ref() else {
            panic!("expected a string, got {:?}", result.as_ref());
        };
        // Trimming a borrowed string keeps borrowing the original bytes.
        assert!(
            matches!(cow, Cow::Borrowed(_)),
            "the trimmed string should stay borrowed"
        );
        assert_eq!(cow.as_ref(), "hi");
    }

    #[test]
    fn owned_strings_are_trimmed() {
        let result = Trim.call(&[Cow::Owned(FilterValue::String(Cow::Owned(
            "  hi there  ".to_string(),
        )))]);
        assert_eq!(result.as_ref(), &FilterValue::String("hi there".into()));
    }

    #[test]
    fn non_strings_yield_null() {
        let result = Trim.call(&[Cow::Owned(FilterValue::Number(1.0))]);
        assert_eq!(result.as_ref(), &FilterValue::Null);
    }

    #[cfg(feature = "secrecy")]
    #[test]
    fn secrets_stay_wrapped_and_redacted() {
        // A secret is trimmed like a string, but the result stays a secret so it
        // can never leak through formatting.
        let result = Trim.call(&[Cow::Owned(FilterValue::secret("  hunter2  "))]);
        let result = result.as_ref();

        assert!(matches!(result, FilterValue::Secret(_)));
        assert_eq!(result.to_string(), "[REDACTED]");
        // The trimmed secret compares equal to the trimmed plaintext.
        assert_eq!(result, &FilterValue::String("hunter2".into()));
    }
}
