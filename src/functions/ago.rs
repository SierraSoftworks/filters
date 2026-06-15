use std::borrow::Cow;

use crate::FilterValue;

function! {
    /// The `ago(duration)` function: the point in time exactly `duration` before the
    /// current UTC time, evaluated afresh on every
    /// [`Filter::matches`](crate::Filter::matches) call. It is the equivalent of
    /// `now() - duration`, so `ago(30m)` is the moment thirty minutes ago.
    ///
    /// A non-duration argument yields [`FilterValue::Null`], consistent with the
    /// language's lenient handling of type mismatches.
    ///
    /// *Only available when the `chrono` crate feature is enabled.*
    ago(duration) {
        match duration.as_ref() {
            FilterValue::Duration(d) => match chrono::Utc::now().checked_sub_signed(*d) {
                Some(dt) => Cow::Owned(FilterValue::DateTime(dt)),
                None => Cow::Owned(FilterValue::Null),
            },
            _ => Cow::Owned(FilterValue::Null),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use crate::{FilterValue, functions::Function};

    use super::ago;

    #[test]
    fn name_and_arity() {
        assert_eq!(ago.name(), "ago");
        assert_eq!(ago.arity(), 1);
    }

    #[test]
    fn returns_a_datetime_in_the_past() {
        let before = chrono::Utc::now();
        let result = ago.call(&[Cow::Owned(FilterValue::Duration(
            chrono::Duration::minutes(30),
        ))]);
        let after = chrono::Utc::now();

        let FilterValue::DateTime(dt) = result.as_ref() else {
            panic!("expected a datetime, got {:?}", result.as_ref());
        };
        // `ago(30m)` sits thirty minutes before "now", which itself lies between
        // the two samples we took around the call.
        assert!(*dt >= before - chrono::Duration::minutes(30));
        assert!(*dt <= after - chrono::Duration::minutes(30));
    }

    #[test]
    fn non_durations_yield_null() {
        let result = ago.call(&[Cow::Owned(FilterValue::Number(1.0))]);
        assert_eq!(result.as_ref(), &FilterValue::Null);
    }
}
