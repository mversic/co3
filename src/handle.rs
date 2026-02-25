//! Utilities for defining opaque pointer handles and shared handle logic.

/// Type of the handle identifier
pub type Id = u8;

/// Represents an opaque handle in an FFI context
///
/// # Safety
///
/// If two structures implement the same id, it may result in a void pointer cast to a wrong type
///
/// Prefer [`crate::handles!`] for assigning IDs.
pub unsafe trait Handle {
    /// Unique identifier of the handle. Most commonly, it is
    /// used to facilitate generic monomorphization over FFI
    const ID: Id;
}

/// Implements [`Handle`] for a list of types.
///
/// ID assignment follows Rust fieldless enum discriminant rules:
/// - first entry without `= ...` gets `0`
/// - each following implicit entry gets previous ID + 1
/// - explicit `= ...` resets the running value for following entries
///
/// # Example
///
/// ```rust
/// struct Foo1;
/// struct Foo2;
/// struct Bar1;
/// struct Bar2;
///
/// co3::handles! {Foo1, Foo2 = 8, Bar1, Bar2}
///
/// /* will produce:
/// impl Handle for Foo1 {
///     const ID: Id = 0;
/// }
/// impl Handle for Foo2 {
///     const ID: Id = 8;
/// }
/// impl Handle for Bar1 {
///     const ID: Id = 9;
/// }
/// impl Handle for Bar2 {
///     const ID: Id = 10;
/// } */
/// ```
#[macro_export]
macro_rules! handles {
    ( @next $next:expr; ) => {};
    ( @next $next:expr; , $($rest:tt)* ) => {
        $crate::handles! { @next $next; $($rest)* }
    };

    ( @next $next:expr; $ty:ty = $id:expr, $($rest:tt)* ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: $crate::handle::Id = $id;
        }
        $crate::handles! { @next ($id) + 1; $($rest)* }
    };
    ( @next $next:expr; $ty:ty = $id:expr $(,)? ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: $crate::handle::Id = $id;
        }
    };

    ( @next $next:expr; $ty:ty, $($rest:tt)* ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: $crate::handle::Id = $next;
        }
        $crate::handles! { @next ($next) + 1; $($rest)* }
    };
    ( @next $next:expr; $ty:ty $(,)? ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: $crate::handle::Id = $next;
        }
    };

    ( $($decls:tt)* ) => {
        $crate::handles! { @next 0; $($decls)* }
    };
}
