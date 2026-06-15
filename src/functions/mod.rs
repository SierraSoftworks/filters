//! Built-in functions, and the [`Function`] trait for defining your own.
//!
//! Filters may call functions using the `name(args...)` syntax. The crate ships
//! a small base set (see [`base_functions`]) — currently [`Trim`] and, with the
//! `chrono` feature, [`Now`], [`Ago`], and [`DateTimeFn`] — but you can add your
//! own by implementing
//! [`Function`] and passing instances to
//! [`Filter::with_functions`](crate::Filter::with_functions).

use std::borrow::Cow;
use std::sync::{Arc, LazyLock};

use crate::FilterValue;

#[macro_use]
mod macros;

pub(crate) mod trim;

#[cfg(feature = "chrono")]
mod ago;
#[cfg(feature = "chrono")]
mod datetime;
#[cfg(feature = "chrono")]
mod now;

/// A function which can be called from within a filter expression using the
/// familiar `name(args...)` syntax.
///
/// The crate provides a small set of built-in functions, but you can define
/// your own by implementing this trait and registering instances with
/// [`Filter::with_functions`](crate::Filter::with_functions). A function
/// reports its [`name`](Function::name) and [`arity`](Function::arity) so the
/// parser can resolve and validate calls up-front, and evaluates via
/// [`call`](Function::call).
///
/// Implementations must be `Send + Sync` so that a parsed [`Filter`](crate::Filter)
/// can be shared across threads.
///
/// ```
/// use std::borrow::Cow;
/// use filt_rs::{FilterValue, Function};
///
/// /// A `len(string)` function returning the number of characters in its argument.
/// struct Len;
///
/// impl Function for Len {
///     fn name(&self) -> &str {
///         "len"
///     }
///
///     fn arity(&self) -> usize {
///         1
///     }
///
///     fn call<'a>(&self, args: &[Cow<'a, FilterValue<'a>>]) -> Cow<'a, FilterValue<'a>> {
///         match args[0].as_ref() {
///             FilterValue::String(s) => Cow::Owned(FilterValue::Number(s.chars().count() as f64)),
///             _ => Cow::Owned(FilterValue::Null),
///         }
///     }
/// }
///
/// assert_eq!(Len.name(), "len");
/// assert_eq!(Len.arity(), 1);
/// ```
pub trait Function: Send + Sync {
    /// The name used to invoke this function in a filter expression.
    fn name(&self) -> &str;

    /// The exact number of arguments this function accepts.
    ///
    /// The parser checks a call's argument count against this when the filter is
    /// parsed, so a mismatch fails fast with a friendly error rather than at
    /// evaluation time.
    fn arity(&self) -> usize;

    /// Evaluates the function against its already-evaluated arguments.
    ///
    /// The slice is guaranteed to hold exactly [`arity`](Function::arity)
    /// elements (the parser enforces this), so implementations may index into it
    /// directly. Returning a value which *borrows* from the arguments — for
    /// example a trimmed sub-slice of a borrowed string — keeps evaluation
    /// allocation-free, mirroring the rest of the interpreter.
    fn call<'a>(&self, args: &[Cow<'a, FilterValue<'a>>]) -> Cow<'a, FilterValue<'a>>;
}

/// Returns the shared set of built-in functions available in every filter,
/// regardless of how it is constructed.
///
/// The set is built once and shared (reference-counted) thereafter, so cloning
/// the returned handle is cheap.
pub(crate) fn base_functions() -> Arc<[Arc<dyn Function>]> {
    static BASE: LazyLock<Arc<[Arc<dyn Function>]>> = LazyLock::new(|| {
        #[allow(unused_mut)]
        let mut functions: Vec<Arc<dyn Function>> = vec![
            Arc::new(trim::trim)
        ];

        #[cfg(feature = "chrono")]
        {
            functions.push(Arc::new(now::now));
            functions.push(Arc::new(ago::ago));
            functions.push(Arc::new(datetime::datetime));
        }

        functions.into()
    });

    BASE.clone()
}
