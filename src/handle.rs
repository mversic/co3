//! Utilities for defining opaque pointer handles and shared handle logic.

use core::ffi::c_void;

#[cfg(feature = "alloc")]
use crate::heapify::Heapify;
use crate::{
    Encode, ExternC,
    borrow::{Borrow, DropFamily, NoDrop},
    boxed::CBox,
    dst::{DstFamily, Sized_},
    ir::{ReprFamily, Transmuted},
    out_ptr::OutPtr,
    transmute::CheckedTransmute,
};

pub trait HandleFamily {
    // TODO: Should Copy be required?
    type Kind: Encode + Copy;
}

/// Represents an opaque handle in an FFI context
///
/// # Safety
///
/// If two structures implement the same id, it may result in a void pointer cast to a wrong type
///
/// Prefer [`crate::handles!`] for assigning IDs.
pub unsafe trait Handle: HandleFamily {
    /// Unique identifier of the handle. Most commonly, it is
    /// used to facilitate generic monomorphization over FFI
    const ID: Self::Kind;
}

#[repr(transparent)]
pub struct Erased(c_void);

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
/// use co3::handles;
///
/// struct Foo1;
/// struct Foo2;
/// struct Bar1;
///
/// # impl co3::handle::HandleFamily for Foo1 {
/// #     type Kind = u8;
/// # }
///
/// # impl co3::handle::HandleFamily for Foo2 {
/// #     type Kind = u8;
/// # }
///
/// # impl co3::handle::HandleFamily for Bar1 {
/// #     type Kind = u8;
/// # }
///
/// handles! {
///     Foo1,
///     Foo2 = 8,
///     Bar1,
/// }
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
/// } */
/// ```
#[macro_export]
macro_rules! handles {
    ( @next $next:expr; ) => {};
    ( @next $next:expr; , $($rest:tt)* ) => {
        $crate::handles! { @next $next; $($rest)* }
    };

    ( @next $next:expr; for<$($lt:lifetime),+> $ty:ty = $id:expr, $($rest:tt)* ) => {
        unsafe impl<$($lt),+> $crate::handle::Handle for $ty {
            const ID: Self::Kind = $id;
        }

        $crate::handles! { @next ($id) + 1; $($rest)* }
    };
    ( @next $next:expr; for<$($lt:lifetime),+> $ty:ty = $id:expr $(,)? ) => {
        unsafe impl<$($lt),+> $crate::handle::Handle for $ty {
            const ID: Self::Kind = $id;
        }
    };

    ( @next $next:expr; for<$($lt:lifetime),+> $ty:ty, $($rest:tt)* ) => {
        unsafe impl<$($lt),+> $crate::handle::Handle for $ty {
            const ID: Self::Kind = $next;
        }

        $crate::handles! { @next ($next) + 1; $($rest)* }
    };
    ( @next $next:expr; for<$($lt:lifetime),+> $ty:ty $(,)? ) => {
        unsafe impl<$($lt),+> $crate::handle::Handle for $ty {
            const ID: Self::Kind = $next;
        }
    };

    ( @next $next:expr; $ty:ty = $id:expr, $($rest:tt)* ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: Self::Kind = $id;
        }

        $crate::handles! { @next ($id) + 1; $($rest)* }
    };
    ( @next $next:expr; $ty:ty = $id:expr $(,)? ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: Self::Kind = $id;
        }
    };

    ( @next $next:expr; $ty:ty, $($rest:tt)* ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: Self::Kind = $next;
        }

        $crate::handles! { @next ($next) + 1; $($rest)* }
    };
    ( @next $next:expr; $ty:ty $(,)? ) => {
        unsafe impl $crate::handle::Handle for $ty {
            const ID: Self::Kind = $next;
        }
    };

    ( $($decls:tt)* ) => {
        $crate::handles! { @next 0; $($decls)* }
    };
}

impl ReprFamily for Erased {
    type Kind = Transmuted;
}

impl DstFamily for Erased {
    type Kind = Sized_;
}

impl DropFamily for Erased {
    type Kind = NoDrop;
}

unsafe impl CheckedTransmute for Erased {
    type Target = c_void;

    #[inline(always)]
    fn is_valid(_: &Self::Target) -> bool {
        true
    }
}

impl ExternC for Erased {
    type CType = CBox<c_void>;
}

impl OutPtr for Erased {
    type OutPtr = CBox<c_void>;
}

impl<const IN_STRUCT: bool> Borrow<IN_STRUCT> for Erased {
    type Borrowed<'itm>
        = Self
    where
        Self: 'itm;

    type Store = ();

    fn borrow<'itm>(self, (): &'itm mut ()) -> Self::Borrowed<'itm>
    where
        Self: 'itm,
    {
        unimplemented!()
    }
}

#[cfg(feature = "alloc")]
impl Heapify for Erased {
    type Kind = Self;

    #[inline(always)]
    fn heapify(self) -> Self::Kind {
        unimplemented!()
    }

    #[inline(always)]
    fn unheapify(_: Self::Kind) -> Self {
        unimplemented!()
    }
}
