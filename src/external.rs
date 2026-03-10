use core::{marker::PhantomData, ptr::NonNull};

use crate::{
    borrow::{Borrow, DropFamily, NoDrop},
    ir::{ReprFamily, Transmuted},
    niche::{Niche, NicheFamily, StableNiche, WithStableNiche},
    transmute::{CheckedTransmute, EncodeTransmuted},
};

/// Represents the pointee on the far side of an exported opaque pointer at the FFI boundary.
///
/// # Safety
///
/// Implementors must guarantee that:
/// - `Self` has the same representation as [`NonNull<Extern>`].
pub unsafe trait External: Sized {
    /// Returns a shared opaque pointer.
    fn as_ptr(&self) -> *const Extern;

    /// Returns a mutable opaque pointer.
    fn as_mut_ptr(&mut self) -> *mut Extern;
}

/// Wrapper around struct/enum opaque pointer. When used through a type declaration in
/// `extern_!`/`extern_C!` in the crate linking dynamically to some `cdylib` crate, it replaces
/// the struct/enum body definition
#[repr(C)]
pub struct Extern {
    __data: [u8; 0],

    // Required for !Send & !Sync & !Unpin.
    //
    // - `*mut u8` is !Send & !Sync. It's wrapped in `PhantomData` not to affect alignment.
    //
    // - `PhantomPinned` is !Unpin. It's wrapped in `PhantomData` because
    //   its memory representation is not guaranteed to be FFI-safe
    __marker: PhantomData<(*mut u8, core::marker::PhantomPinned)>,
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ExternRef<'a, T>(NonNull<Extern>, PhantomData<&'a T>);

#[derive(Clone)]
#[repr(transparent)]
pub struct ExternRefMut<'a, T>(NonNull<Extern>, PhantomData<&'a mut T>);

macro_rules! impl_external_ref_common {
    ($ty:ident, $target:ty, $niche:expr) => {
        impl<R> ReprFamily for $ty<'_, R> {
            type Kind = Transmuted;
        }

        impl<R> NicheFamily for $ty<'_, R> {
            type Kind = WithStableNiche;
        }

        unsafe impl<R> CheckedTransmute for $ty<'_, R> {
            type Target = $target;

            #[inline(always)]
            fn is_valid(target: &Self::Target) -> bool {
                !target.is_null()
            }
        }

        impl<R> Niche for $ty<'_, R> {
            const NICHE_VALUE: Self::CType = $niche;
        }

        unsafe impl<R> StableNiche for $ty<'_, R> {}

        impl<R> DropFamily for $ty<'_, R> {
            type Kind = NoDrop;
        }

        impl<'a, R> Borrow for $ty<'a, R> {
            type Store = ();

            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            fn borrow<'itm>(self, (): &'itm mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }

        unsafe impl<R> EncodeTransmuted for $ty<'_, R>
        where
            Self: CheckedTransmute<Target: crate::Encode>,
        {
            type Store = <Self::Target as crate::Encode>::Store;
        }

        impl<T> core::ops::Deref for $ty<'_, T> {
            type Target = T;

            fn deref(&self) -> &Self::Target {
                let ptr: *const _ = &self.0.as_ptr();
                unsafe { &*(ptr.cast::<T>()) }
            }
        }
    };
}

impl<T: External> ExternRef<'_, T> {
    pub fn new(inner: &T) -> Self {
        let value = unsafe { NonNull::new_unchecked(inner.as_ptr() as *mut _) };

        Self(value, PhantomData)
    }
}

impl<T: External> ExternRefMut<'_, T> {
    pub fn new(inner: &mut T) -> Self {
        let value = unsafe { NonNull::new_unchecked(inner.as_mut_ptr()) };

        Self(value, PhantomData)
    }
}

impl<T> core::ops::DerefMut for ExternRefMut<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        let ptr: *mut _ = &mut self.0.as_ptr();
        unsafe { &mut *(ptr.cast::<T>()) }
    }
}

impl_external_ref_common!(ExternRef, *const Extern, core::ptr::null());
impl_external_ref_common!(ExternRefMut, *mut Extern, core::ptr::null_mut());
