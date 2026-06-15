use std::borrow::Cow;

use crate::FilterValue;

function!{
    /// The `now()` function: the current UTC time, evaluated afresh on every
    /// [`Filter::matches`](crate::Filter::matches) call so that each evaluation sees
    /// the current time.
    ///
    /// *Only available when the `chrono` crate feature is enabled.*
    now() {
        Cow::Owned(FilterValue::DateTime(chrono::Utc::now()))
    }
}