use std::borrow::Cow;

#[cfg(feature = "secrecy")]
use secrecy::ExposeSecret;

use crate::FilterValue;

function! {
    /// The `datetime(string)` function: parses an ISO 8601 / RFC 3339 string into an
    /// absolute point in time, e.g. `datetime("2026-03-12T12:00:00")`.
    ///
    /// A string carrying an explicit offset (e.g. `2026-03-12T12:00:00Z` or
    /// `+02:00`) is honoured and converted to UTC; a string without an offset is
    /// interpreted as UTC. Any argument which is not a string, or a string which
    /// fails to parse, yields [`FilterValue::Null`], consistent with the language's
    /// lenient handling of type mismatches.
    ///
    /// *Only available when the `chrono` crate feature is enabled.*
    datetime(string) {
        let parsed = match string.as_ref() {
            FilterValue::String(s) => parse(s),
            #[cfg(feature = "secrecy")]
            FilterValue::Secret(s) => parse(s.expose_secret()),
            _ => None,
        };

        match parsed {
            Some(dt) => Cow::Owned(FilterValue::DateTime(dt)),
            None => Cow::Owned(FilterValue::Null),
        }
    }
}

/// Parses an ISO 8601 / RFC 3339 string into a UTC datetime.
///
/// A string with an explicit offset is parsed via RFC 3339 and converted to
/// UTC; a string without an offset is parsed as a naive datetime and assumed to
/// be UTC. Returns [`None`] if the string matches neither form.
fn parse(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }

    s.parse::<chrono::NaiveDateTime>()
        .ok()
        .map(|naive| naive.and_utc())
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use chrono::{TimeZone, Utc};

    use crate::{FilterValue, functions::Function};

    use super::datetime;

    #[test]
    fn name_and_arity() {
        assert_eq!(datetime.name(), "datetime");
        assert_eq!(datetime.arity(), 1);
    }

    #[test]
    fn parses_a_naive_iso8601_string_as_utc() {
        let result = datetime.call(&[Cow::Owned(FilterValue::String(Cow::Borrowed(
            "2026-03-12T12:00:00",
        )))]);
        assert_eq!(
            result.as_ref(),
            &FilterValue::DateTime(Utc.with_ymd_and_hms(2026, 3, 12, 12, 0, 0).unwrap())
        );
    }

    #[test]
    fn honours_an_explicit_offset() {
        // 12:00 at +02:00 is 10:00 UTC.
        let result = datetime.call(&[Cow::Owned(FilterValue::String(Cow::Borrowed(
            "2026-03-12T12:00:00+02:00",
        )))]);
        assert_eq!(
            result.as_ref(),
            &FilterValue::DateTime(Utc.with_ymd_and_hms(2026, 3, 12, 10, 0, 0).unwrap())
        );
    }

    #[test]
    fn invalid_strings_yield_null() {
        let result = datetime.call(&[Cow::Owned(FilterValue::String(Cow::Borrowed(
            "not a datetime",
        )))]);
        assert_eq!(result.as_ref(), &FilterValue::Null);
    }

    #[test]
    fn non_strings_yield_null() {
        let result = datetime.call(&[Cow::Owned(FilterValue::Number(1.0))]);
        assert_eq!(result.as_ref(), &FilterValue::Null);
    }
}
