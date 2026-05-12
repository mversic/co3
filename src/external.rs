use core::{ffi::c_void, marker::PhantomData, ptr::NonNull};

use crate::{
    borrow::{Borrow, ToOwned},
    heapify::Heapify,
    ir::{ReprFamily, Transmuted},
    niche::{Niche, NicheFamily, StableNiche, WithStableNiche},
    transmute::{CheckedTransmute, EncodeTransmuted},
};

/// Represents the pointee on the far side of an exported opaque pointer at the FFI boundary.
///
/// # Safety
///
/// Implementors must guarantee that:
/// - `Self` has the same representation as [`NonNull<c_void>`].
pub unsafe trait External: Sized {
    /// Returns a shared opaque pointer.
    fn as_ptr(&self) -> *const c_void;

    /// Returns a mutable opaque pointer.
    fn as_mut_ptr(&mut self) -> *mut c_void;
}

#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct ExternRef<'a, T>(NonNull<c_void>, PhantomData<&'a T>);

#[derive(Clone)]
#[repr(transparent)]
pub struct ExternRefMut<'a, T>(NonNull<c_void>, PhantomData<&'a mut T>);

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

        impl<R> crate::size::SizeFamily for $ty<'_, R> {
            type Kind = crate::size::SizedType;
        }

        impl<R> Heapify for $ty<'_, R> {
            type Kind = Self;

            #[inline(always)]
            fn heapify(self) -> Self::Kind {
                self
            }

            #[inline(always)]
            fn unheapify(kind: Self::Kind) -> Self {
                kind
            }
        }

        impl<R, const IN_STRUCT: bool> Borrow<IN_STRUCT> for $ty<'_, R> {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Store = ();

            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }

        impl<'r, R, const IN_STRUCT: bool> ToOwned<'r, IN_STRUCT> for $ty<'r, R> {
            fn to_owned(borrowed: Self::Borrowed<'r>) -> Self {
                borrowed
            }
        }

        unsafe impl<R> EncodeTransmuted for $ty<'_, R>
        where
            Self: CheckedTransmute<Target: crate::EncodeWithStore>,
        {
            type Store = <Self::Target as crate::EncodeWithStore>::Store;
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

impl_external_ref_common!(ExternRef, *const c_void, core::ptr::null());
impl_external_ref_common!(ExternRefMut, *mut c_void, core::ptr::null_mut());
