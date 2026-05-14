//! Logic related to the conversion of slices to and from FFI-compatible representation

use crate::{ReprC, borrow::BorrowCast, reprC};

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

impl_raw_slice_methods! { CSlice<C>, CSliceMut<C> }

impl<C> CSlice<C> {
    /// Set the slice's data pointer to null
    pub const fn none() -> Self {
        Self {
            data: core::ptr::null(),
            len: 0,
        }
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

    /// Create [`Self`] from a raw data pointer and slice metadata.
    pub(crate) const fn from_raw_parts(data: *const C, len: usize) -> Self {
        Self { data, len }
    }

    pub(crate) const fn as_ptr(&self) -> *const C {
        self.data
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
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

    /// Create [`Self`] from a raw data pointer and slice metadata.
    pub(crate) const fn from_raw_parts_mut(data: *mut C, len: usize) -> Self {
        Self { data, len }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut C {
        self.data
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }
}

impl<C: ReprC> CSlice<C> {
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

        Some(unsafe { core::slice::from_raw_parts(self.data, self.len) })
    }
}

impl<C: ReprC> CSliceMut<C> {
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

        Some(unsafe { core::slice::from_raw_parts_mut(self.data, self.len) })
    }
}

reprC! {
    unsafe impl(C: ReprC) SizedRobust for CSlice<C> {}
}
unsafe impl<C: ReprC> BorrowCast for CSlice<C> {
    type AsConst = Self;
    type AsMut = Self;
}

reprC! {
    unsafe impl(C: ReprC) SizedRobust for CSliceMut<C> {}
}
unsafe impl<C: ReprC> BorrowCast for CSliceMut<C> {
    type AsConst = Self;
    type AsMut = Self;
}
