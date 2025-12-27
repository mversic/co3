//! Logic related to the conversion of slices to and from FFI-compatible representation

use alloc::{boxed::Box, vec::Vec};
use core::slice;

use crate::ReprC;

crate::decl_fns! { dealloc }

/// Immutable slice `&[C]` with a defined C ABI layout. Consists of a data pointer and a length.
/// If the data pointer is set to `null`, the struct represents `Option<&[C]>`.
#[repr(C)]
pub struct RawSlice<C>(*const C, usize);

/// Mutable slice `&mut [C]` with a defined C ABI layout. Consists of a data pointer and a length.
/// If the data pointer is set to `null`, the struct represents `Option<&mut [C]>`.
#[repr(C)]
pub struct RawSliceMut<C>(*mut C, usize);

/// Owned slice `Box<[C]>` with a defined C ABI layout. Consists of a data pointer and a length.
/// Used in place of a function out-pointer to transfer ownership of the slice to the caller.
/// If the data pointer is set to `null`, the struct represents `Option<Box<[C]>>`.
#[repr(C)]
pub struct OutBoxedSlice<C>(*mut C, usize);

macro_rules! impl_raw_slice_methods {
    ($($ty:ty),+ $(,)?) => {$(
        impl<C> core::fmt::Debug for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                if self.0.is_null() {
                    f.debug_struct(stringify!($ty))
                        .field("ptr", &self.0)
                        .finish_non_exhaustive()
                } else {
                    f.debug_struct(stringify!($ty))
                        .field("ptr", &self.0)
                        .field("len", &self.1)
                        .finish()
                }
            }
        }
        impl<C> PartialEq for $ty {
            fn eq(&self, other: &Self) -> bool {
                match (self.0.is_null(), other.0.is_null()) {
                    (true, true) => true,
                    (false, false) => {
                        if self.1 == 0 || other.1 == 0 {
                            self.1 == other.1
                        } else {
                            self.0 == other.0 && self.1 == other.1
                        }
                    }
                    _ => false,
                }
            }
        }
        impl<C> Eq for $ty {}
        impl<C> PartialOrd for $ty {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl<C> Ord for $ty {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                use core::cmp::Ordering;
                match (self.0.is_null(), other.0.is_null()) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => {
                        if self.1 == 0 || other.1 == 0 {
                            self.1.cmp(&other.1)
                        } else {
                            match self.0.cmp(&other.0) {
                                Ordering::Equal => self.1.cmp(&other.1),
                                ordering => ordering,
                            }
                        }
                    }
                }
            }
        }
        impl<C> Clone for $ty {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<C> Copy for $ty {})+
    };
}

// NOTE: derive impls regardles of whether `C` implements `ReprC`
impl_raw_slice_methods! { RawSlice<C>, RawSliceMut<C>, OutBoxedSlice<C> }

impl<C> RawSlice<C> {
    /// Set the slice's data pointer to null
    pub const fn none() -> Self {
        Self(core::ptr::null(), 0)
    }

    /// Create a slice from a data pointer and a length.
    pub const fn from_raw_parts(ptr: *const C, len: usize) -> Self {
        Self(ptr, len)
    }

    /// Create [`Self`] from shared slice
    pub const fn from_slice(source: Option<&[C]>) -> Self {
        if let Some(slice) = source {
            return Self(slice.as_ptr(), slice.len());
        }

        Self::none()
    }

    /// Convert [`Self`] into a shared slice. Return `None` if data pointer is null.
    /// Unlike [`core::slice::from_raw_parts`], data pointer is allowed to be null.
    ///
    /// # Safety
    ///
    /// Check [`core::slice::from_raw_parts`]
    pub const unsafe fn into_rust<'slice>(self) -> Option<&'slice [C]> {
        if self.0.is_null() {
            return None;
        }

        Some(unsafe { slice::from_raw_parts(self.0, self.1) })
    }
}
impl<C> RawSliceMut<C> {
    /// Set the slice's data pointer to null
    pub const fn none() -> Self {
        Self(core::ptr::null_mut(), 0)
    }

    /// Create a slice from a data pointer and a length.
    pub const fn from_raw_parts_mut(ptr: *mut C, len: usize) -> Self {
        Self(ptr, len)
    }

    /// Create [`Self`] from mutable slice
    pub const fn from_slice(source: Option<&mut [C]>) -> Self {
        if let Some(slice) = source {
            return Self(slice.as_mut_ptr(), slice.len());
        }

        Self::none()
    }

    /// Convert [`Self`] into a mutable slice. Return `None` if data pointer is null.
    /// Unlike [`core::slice::from_raw_parts_mut`], data pointer is allowed to be null.
    ///
    /// # Safety
    ///
    /// Check [`core::slice::from_raw_parts_mut`]
    pub const unsafe fn into_rust<'slice>(self) -> Option<&'slice mut [C]> {
        if self.0.is_null() {
            return None;
        }

        Some(unsafe { slice::from_raw_parts_mut(self.0, self.1) })
    }
}
impl<C: ReprC> OutBoxedSlice<C> {
    /// Set the slice's data pointer to null
    const fn none() -> Self {
        Self(core::ptr::null_mut(), 0)
    }

    /// Create a slice from a data pointer and a length.
    pub const fn from_raw_parts(ptr: *mut C, len: usize) -> Self {
        Self(ptr, len)
    }

    /// Create [`Self`] from a [`Box<[T]>`]
    pub fn from_boxed_slice(source: Option<Box<[C]>>) -> Self {
        if let Some(boxed_slice) = source {
            let mut boxed_slice = core::mem::ManuallyDrop::new(boxed_slice);
            return Self(boxed_slice.as_mut_ptr(), boxed_slice.len());
        }

        Self::none()
    }

    /// Create a `Vec<T>` directly from the raw components of another vector.
    /// Unlike [`Vec::from_raw_parts`], data pointer is allowed to be null.
    ///
    /// # Safety
    ///
    /// Check [`Vec::from_raw_parts`]
    pub unsafe fn into_rust(self) -> Option<Vec<C>> {
        if self.0.is_null() {
            return None;
        }

        Some(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(self.0, self.1)).to_vec() })
    }

    pub(crate) unsafe fn deallocate(&self) -> bool {
        if self.0.is_null() {
            return true;
        }
        if self.1 == 0 {
            return true;
        }

        if let Ok(layout) = core::alloc::Layout::array::<C>(self.1) {
            unsafe {
                __dealloc(self.0.cast(), layout.size(), layout.align());
            }

            return true;
        }

        false
    }
}

impl<C: ReprC> From<OutBoxedSlice<C>> for RawSliceMut<C> {
    fn from(slice: OutBoxedSlice<C>) -> Self {
        Self::from_raw_parts_mut(slice.0, slice.1)
    }
}

// SAFETY: Robust type with a defined C ABI
unsafe impl<T: ReprC> ReprC for RawSlice<T> {}
// SAFETY: Robust type with a defined C ABI
unsafe impl<T: ReprC> ReprC for RawSliceMut<T> {}
// SAFETY: Robust type with a defined C ABI
unsafe impl<T: ReprC> ReprC for OutBoxedSlice<T> {}
