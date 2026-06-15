use std::borrow::Cow;

use crate::FilterValue;

use super::Function;

/// The `now()` function: the current UTC time, evaluated afresh on every
/// [`Filter::matches`](crate::Filter::matches) call so that each evaluation sees
/// the current time.
///
/// *Only available when the `chrono` crate feature is enabled.*
pub(crate) struct Now;

impl Function for Now {
    fn name(&self) -> &str {
        "now"
    }

    fn arity(&self) -> usize {
        0
    }

    fn call<'a>(&self, _args: &[Cow<'a, FilterValue<'a>>]) -> Cow<'a, FilterValue<'a>> {
        Cow::Owned(FilterValue::DateTime(chrono::Utc::now()))
    }
}
