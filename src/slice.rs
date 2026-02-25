//! Logic related to the conversion of slices to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc::boxed::Box;
use core::slice;

use crate::{ReprC, repr_C};

type DeallocFn = unsafe extern "C" fn(*mut u8, usize, usize) -> crate::FfiReturn;

/// Immutable slice `&[C]` with a defined C ABI layout. Consists of a data pointer and a length.
/// If the data pointer is set to `null`, the struct represents `Option<&[C]>`.
#[repr(C)]
pub struct CSlice<C> {
    data: *const C,
    len: usize,
}

/// Mutable slice `&mut [C]` with a defined C ABI layout. Consists of a data pointer and a length.
/// If the data pointer is set to `null`, the struct represents `Option<&mut [C]>`.
#[repr(C)]
pub struct CSliceMut<C> {
    data: *mut C,
    len: usize,
}

/// Owned slice `Box<[C]>` with a defined C ABI layout. Consists of a data pointer and a length.
/// Used in place of a function out-pointer to transfer ownership of the slice to the caller.
/// If the data pointer is set to `null`, the struct represents `Option<Box<[C]>>`.
#[repr(C)]
pub struct CBoxedSlice<C> {
    data: *mut C,
    len: usize,
    dealloc: Option<DeallocFn>,
}

macro_rules! impl_raw_slice_methods {
    ($($ty:ty),+ $(,)?) => {$(
        impl<C> core::fmt::Debug for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                if self.data.is_null() {
                    f.debug_struct(stringify!($ty))
                        .field("data", &self.data)
                        .finish_non_exhaustive()
                } else {
                    f.debug_struct(stringify!($ty))
                        .field("data", &self.data)
                        .field("len", &self.len)
                        .finish()
                }
            }
        }
        impl<C> PartialEq for $ty {
            fn eq(&self, other: &Self) -> bool {
                match (self.data.is_null(), other.data.is_null()) {
                    (true, true) => true,
                    (false, false) => {
                        if self.len == 0 || other.len == 0 {
                            self.len == other.len
                        } else {
                            self.data == other.data && self.len == other.len
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
                match (self.data.is_null(), other.data.is_null()) {
                    (true, true) => Ordering::Equal,
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    (false, false) => {
                        if self.len == 0 || other.len == 0 {
                            self.len.cmp(&other.len)
                        } else {
                            match self.data.cmp(&other.data) {
                                Ordering::Equal => self.len.cmp(&other.len),
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
impl_raw_slice_methods! { CSlice<C>, CSliceMut<C>, CBoxedSlice<C> }

impl<C> CSlice<C> {
    /// Set the slice's data pointer to null
    pub const fn none() -> Self {
        Self {
            data: core::ptr::null(),
            len: 0,
        }
    }

    /// Create a slice from a data pointer and a length.
    pub const fn from_raw_parts(data: *const C, len: usize) -> Self {
        Self { data, len }
    }

    /// Create [`Self`] from shared slice
    pub const fn from_slice(source: Option<&[C]>) -> Self {
        if let Some(slice) = source {
            return Self {
                data: slice.as_ptr(),
                len: slice.len(),
            };
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
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { slice::from_raw_parts(self.data, self.len) })
    }
}
impl<C> CSliceMut<C> {
    /// Set the slice's data pointer to null
    pub const fn none() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len: 0,
        }
    }

    /// Create a slice from a data pointer and a length.
    pub const fn from_raw_parts_mut(data: *mut C, len: usize) -> Self {
        Self { data, len }
    }

    /// Create [`Self`] from mutable slice
    pub const fn from_slice(source: Option<&mut [C]>) -> Self {
        if let Some(slice) = source {
            return Self {
                data: slice.as_mut_ptr(),
                len: slice.len(),
            };
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
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { slice::from_raw_parts_mut(self.data, self.len) })
    }
}
impl<C: ReprC> CBoxedSlice<C> {
    /// Set the slice's data pointer to null
    const fn none() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len: 0,
            dealloc: None,
        }
    }

    /// Create [`Self`] from a [`Box<[T]>`]
    #[cfg(feature = "alloc")]
    pub fn from_boxed_slice(source: Option<Box<[C]>>, dealloc: DeallocFn) -> Self {
        let mut boxed_slice = core::mem::ManuallyDrop::new(source);

        let Some(boxed_slice) = boxed_slice.as_deref_mut() else {
            return Self::none();
        };

        Self {
            data: boxed_slice.as_mut_ptr(),
            len: boxed_slice.len(),
            dealloc: Some(dealloc),
        }
    }

    /// Convert [`Self`] into a boxed slice. Return `None` if data pointer is null.
    /// Unlike [`Box::from_raw`], data pointer is allowed to be null.
    ///
    /// # Safety
    ///
    /// Check [`Box::from_raw`]
    #[cfg(feature = "alloc")]
    pub unsafe fn into_rust(self) -> Option<Box<[C]>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(self.data, self.len)) })
    }

    pub(crate) unsafe fn deallocate(&self) -> bool {
        if self.data.is_null() || self.len == 0 {
            return true;
        }

        let dealloc = self.dealloc.unwrap();
        if let Ok(layout) = core::alloc::Layout::array::<C>(self.len) {
            unsafe {
                dealloc(self.data.cast(), layout.size(), layout.align());
            }

            return true;
        }

        false
    }
}

impl<C: ReprC> From<CBoxedSlice<C>> for CSliceMut<C> {
    fn from(slice: CBoxedSlice<C>) -> Self {
        Self::from_raw_parts_mut(slice.data, slice.len)
    }
}

repr_C! {
    unsafe impl(T: ReprC) Robust for CSlice<T> {}
}
repr_C! {
    unsafe impl(T: ReprC) Robust for CSliceMut<T> {}
}
repr_C! {
    unsafe impl(T: ReprC) Robust for CBoxedSlice<T> {}
}
