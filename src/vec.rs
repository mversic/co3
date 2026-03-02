//! Logic related to the conversion of vectors to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc_crate::vec::Vec;

#[cfg(feature = "alloc")]
use crate::alloc::Global;
use crate::{ReprC, alloc::Allocator, reprC, slice::CSliceMut};

/// Owned vector `Vec<C>` with a defined C ABI layout and a deallocate function.
///
/// If the data pointer is set to `null`, the struct represents `Option<Vec<C>>`.
#[repr(C)]
pub struct CVec<C, A: Allocator = Global> {
    data: *mut C,
    len: usize,
    cap: usize,
    allocator: A,
}

impl<C, A: Allocator> core::fmt::Debug for CVec<C, A> {
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
impl<C, A: Allocator> PartialEq for CVec<C, A> {
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
impl<C, A: Allocator> Eq for CVec<C, A> {}
impl<C, A: Allocator> PartialOrd for CVec<C, A> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C, A: Allocator> Ord for CVec<C, A> {
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
impl<C, A: Allocator> Clone for CVec<C, A> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C, A: Allocator> Copy for CVec<C, A> {}

#[cfg(feature = "alloc")]
impl<C> CVec<C> {
    /// Create [`Self`] from a [`Vec<T>`].
    pub fn from_vec(source: Option<Vec<C>>) -> Self {
        if let Some(vec) = source {
            let mut vec = core::mem::ManuallyDrop::new(vec);

            return Self {
                data: vec.as_mut_ptr(),
                len: vec.len(),
                cap: vec.capacity(),
                allocator: crate::alloc::Global,
            };
        }

        Self::none()
    }
}

impl<C, A: Allocator> CVec<C, A> {
    /// Set the vector's data pointer to null
    pub const fn none() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len: 0,
            cap: 0,
            // SAFETY: allocator will never be used
            allocator: unsafe { core::mem::zeroed() },
        }
    }
}

#[cfg(feature = "alloc")]
impl<C: ReprC, A: Allocator> CVec<C, A> {
    /// Convert [`Self`] into a vector. Return `None` if data pointer is null.
    ///
    /// # Safety
    ///
    /// Check [`Vec::from_raw_parts`]
    pub unsafe fn into_rust(self) -> Option<Vec<C>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { Vec::from_raw_parts(self.data, self.len, self.cap) })
    }
}

impl<C, A: Allocator> From<CVec<C, A>> for CSliceMut<C> {
    fn from(vec: CVec<C, A>) -> Self {
        Self::from_raw_parts_mut(vec.data, vec.len)
    }
}

reprC! {
    unsafe impl(T: ReprC, A: Allocator) Robust for CVec<T, A> {}
}
