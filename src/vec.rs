//! Logic related to the conversion of vectors to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc::vec::Vec;
use core::mem::ManuallyDrop;

use crate::ReprC;

crate::decl_fns! { dealloc }

/// Owned vector `Vec<C>` with a defined C ABI layout. Consists of a data pointer, a length, and a capacity.
/// If the data pointer is set to `null`, the struct represents `Option<Vec<C>>`.
#[repr(C)]
pub struct CVec<C> {
    data: *mut C,
    len: usize,
    cap: usize,
}

impl<C> core::fmt::Debug for CVec<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.data.is_null() {
            f.debug_struct(stringify!(CVec))
                .field("data", &self.data)
                .finish_non_exhaustive()
        } else {
            f.debug_struct(stringify!(CVec))
                .field("data", &self.data)
                .field("len", &self.len)
                .field("cap", &self.cap)
                .finish()
        }
    }
}
impl<C> PartialEq for CVec<C> {
    fn eq(&self, other: &Self) -> bool {
        match (self.data.is_null(), other.data.is_null()) {
            (true, true) => true,
            (false, false) => {
                if self.len == 0 || other.len == 0 {
                    self.len == other.len && self.cap == other.cap
                } else {
                    self.data == other.data && self.len == other.len && self.cap == other.cap
                }
            }
            _ => false,
        }
    }
}
impl<C> Eq for CVec<C> {}
impl<C> PartialOrd for CVec<C> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C> Ord for CVec<C> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        use core::cmp::Ordering;
        match (self.data.is_null(), other.data.is_null()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => {
                if self.len == 0 || other.len == 0 {
                    match self.len.cmp(&other.len) {
                        Ordering::Equal => self.cap.cmp(&other.cap),
                        ordering => ordering,
                    }
                } else {
                    match self.data.cmp(&other.data) {
                        Ordering::Equal => match self.len.cmp(&other.len) {
                            Ordering::Equal => self.cap.cmp(&other.cap),
                            ordering => ordering,
                        },
                        ordering => ordering,
                    }
                }
            }
        }
    }
}
impl<C> Clone for CVec<C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C> Copy for CVec<C> {}

impl<C> CVec<C> {
    /// Set the vector's data pointer to null
    pub const fn none() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len: 0,
            cap: 0,
        }
    }

    /// Create a vector from a data pointer, a length, and a capacity.
    pub const fn from_raw_parts(data: *mut C, len: usize, cap: usize) -> Self {
        Self { data, len, cap }
    }

    /// Create [`Self`] from a [`Vec<T>`].
    #[cfg(feature = "alloc")]
    pub fn from_vec(source: Option<Vec<C>>) -> Self {
        if let Some(vec) = source {
            let mut vec = ManuallyDrop::new(vec);

            return Self {
                data: vec.as_mut_ptr(),
                len: vec.len(),
                cap: vec.capacity(),
            };
        }

        Self::none()
    }

    /// Convert [`Self`] into a vector. Return `None` if data pointer is null.
    ///
    /// # Safety
    ///
    /// Check [`Vec::from_raw_parts`]
    #[cfg(feature = "alloc")]
    pub unsafe fn into_rust(self) -> Option<Vec<C>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { Vec::from_raw_parts(self.data, self.len, self.cap) })
    }
}

// SAFETY: Robust type with a defined C ABI
unsafe impl<T: ReprC> ReprC for CVec<T> {}
