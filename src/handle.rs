//! Utilities for defining opaque pointer handles and shared handle logic.

use core::{ffi::c_void, marker::PhantomData};

use alloc_crate::boxed::Box;
use disjoint_impls::disjoint_impls;

use crate::{
    Encode, ExternC,
    borrow::Borrow,
    boxed::CBox,
    external::Extern,
    ir::{Opaque, ReprFamily, Transmuted},
    out_ptr::OutPtr,
    transmute::CheckedTransmute,
};

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

pub trait HandleFamily {
    // FIXME: Remove Store bound once we have no-store conversion traits
    type Kind: Encode<Store = ()>;
}

#[repr(transparent)]
pub struct Erased<R>(c_void, PhantomData<R>);

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

unsafe impl<R> CheckedTransmute for Erased<R> {
    type Target = c_void;

    #[inline(always)]
    fn is_valid(_: &Self::Target) -> bool {
        true
    }
}

disjoint_impls! {
    #[disjoint_impls(remote)]
    pub trait ExternC: Sized {
        type CType: crate::ReprC;
    }

    impl<R: ReprFamily<Kind = Opaque>> ExternC for Erased<R> {
        type CType = CBox<c_void>;
    }

    impl<R: ReprFamily<Kind = Transmuted>> ExternC for Erased<R>
    where
        // FIXME: It's imprecise to say Extern types are just transmuted
        // It poses a danger because any transmuted type can become erased
        R: ExternC<CType = *mut Extern>,
    {
        type CType = *mut c_void;
    }
}

disjoint_impls! {
    #[disjoint_impls(remote)]
    pub trait Borrow: Sized {
        type Borrowed<'itm> where Self: 'itm;
        type Store: Default;

        fn borrow<'itm>(self, store: &'itm mut Self::Store) -> Self::Borrowed<'itm>
        where
            Self: 'itm;
    }

    impl<R: ReprFamily<Kind = Opaque>> Borrow for Erased<R> {
        type Borrowed<'itm>
            = Self
        where
            Self: 'itm;

        type Store = ();

        fn borrow<'itm>(self, _: &'itm mut ()) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            self
        }
    }
    impl<R: ReprFamily<Kind = Transmuted>> Borrow for Erased<R>
    where
        // FIXME: It's imprecise to say Extern types are just transmuted
        // It poses a danger because any transmuted type can become erased
        R: ExternC<CType = *mut Extern>
    {
        type Borrowed<'itm>
            = Self
        where
            Self: 'itm;

        type Store = ();

        fn borrow<'itm>(self, _: &'itm mut ()) -> Self::Borrowed<'itm>
        where
            Self: 'itm,
        {
            self
        }
    }
}

disjoint_impls! {
    #[disjoint_impls(remote)]
    pub trait OutPtr: ExternC {
        type OutPtr: co3::ReprC;
    }

    impl<R: ReprFamily<Kind = Opaque>> OutPtr for Erased<R> {
        type OutPtr = CBox<c_void>;
    }

    impl<R: ReprFamily<Kind = Transmuted>> OutPtr for Erased<R>
    where
        // FIXME: It's imprecise to say Extern types are just transmuted
        // It poses a danger because any transmuted type can become erased
        R: ExternC<CType = *mut Extern>
    {
        type OutPtr = *mut c_void;
    }
}

impl<R> ReprFamily for Erased<R> {
    type Kind = Transmuted;
}

impl<R> ReprFamily for &Erased<R> {
    type Kind = Transmuted;
}

impl<R> ReprFamily for &mut Erased<R> {
    type Kind = Transmuted;
}

impl<R> ReprFamily for Box<Erased<R>> {
    type Kind = Transmuted;
}
