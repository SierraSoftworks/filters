/// Defines a function type with the given name and argument names, implementing
/// [`Function`] with the provided body.
/// 
/// The body may refer to the arguments by name, and must return a
/// [`Cow<FilterValue>`]. The body is evaluated each time the function is called, and may
/// borrow from the arguments, but must not retain references to them beyond the
/// lifetime of the returned value.
/// 
/// ## Example
/// ```rust
/// use filt_rs::{function, FilterValue};
/// use std::borrow::Cow;
/// 
/// function!{
///     /// Calculates the length of a string argument, returning a number.
///     len(s) {
///         match s.as_ref() {
///             FilterValue::String(s) => Cow::Owned(FilterValue::Number(s.chars().count() as f64)),
///             _ => Cow::Owned(FilterValue::Null),
///         }
///     }
/// }
/// ```
#[macro_export]
macro_rules! function {
    ($(#[$outer:meta])* $ty:ident ( $($arg:ident),* ) $body:block) => {
        #[allow(non_camel_case_types)]
        $(#[$outer])*
        pub(crate) struct $ty;

        impl $crate::Function for $ty {
            fn name(&self) -> &str {
                stringify!($ty)
            }

            fn arity(&self) -> usize {
                function!(!count $($arg)*)
            }

            fn call<'a>(&self, args: &[::std::borrow::Cow<'a, $crate::FilterValue<'a>>]) -> ::std::borrow::Cow<'a, $crate::FilterValue<'a>> {
                let mut iter = args.iter();
                $(let $arg = iter.next().unwrap();)*
                $body
            }
        }
    };

    (!count $x:tt $($xs:tt)*) => {
        1usize $(+ function!(!count $xs))*
    };

    (!count) => {
        0usize
    };
}