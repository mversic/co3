//! Logic related to the conversion of boxed values to and from FFI-compatible representation.

use core::ptr::NonNull;

#[cfg(feature = "alloc")]
use alloc_crate::boxed::Box;

#[cfg(feature = "alloc")]
use crate::alloc::Global;
use crate::{ReprC, alloc::Allocator, reprC};

/// Owned pointer `Box<C>` with a deallocate function.
///
/// If the data pointer is set to `null`, the struct represents `Option<Box<C>>`.
#[repr(C)]
pub struct CBox<C, A: Allocator = Global> {
    data: *mut C,
    allocator: A,
}

/// Owned slice `Box<[C]>` with a defined C ABI layout. Consists of a data pointer and a length.
/// Used in place of a function out-pointer to transfer ownership of the slice to the caller.
/// If the data pointer is set to `null`, the struct represents `Option<Box<[C]>>`.
#[repr(C)]
pub struct CBoxedSlice<C, A: Allocator = Global> {
    data: *mut C,
    len: usize,
    allocator: A,
}

impl<C, A: Allocator> core::fmt::Debug for CBox<C, A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct(stringify!(CBox))
            .field("data", &self.data)
            .finish_non_exhaustive()
    }
}

impl<C, A: Allocator> core::fmt::Debug for CBoxedSlice<C, A> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if self.data.is_null() {
            f.debug_struct(stringify!(CBoxedSlice))
                .field("data", &self.data)
                .finish_non_exhaustive()
        } else {
            f.debug_struct(stringify!(CBoxedSlice))
                .field("data", &self.data)
                .field("len", &self.len)
                .finish()
        }
    }
}

impl<C, A: Allocator> PartialEq for CBox<C, A> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl<C, A: Allocator> PartialEq for CBoxedSlice<C, A> {
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

impl<C, A: Allocator> Eq for CBox<C, A> {}
impl<C, A: Allocator> Eq for CBoxedSlice<C, A> {}

impl<C, A: Allocator> PartialOrd for CBox<C, A> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C, A: Allocator> PartialOrd for CBoxedSlice<C, A> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<C, A: Allocator> Ord for CBox<C, A> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.data.cmp(&other.data)
    }
}
impl<C, A: Allocator> Ord for CBoxedSlice<C, A> {
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

impl<C, A: Allocator> Clone for CBox<C, A> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C, A: Allocator> Clone for CBoxedSlice<C, A> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C, A: Allocator> Copy for CBox<C, A> {}
impl<C, A: Allocator> Copy for CBoxedSlice<C, A> {}

#[cfg(feature = "alloc")]
impl<C> CBox<C> {
    /// Create [`Self`] from a [`Box<C>`].
    pub fn from_box(source: Option<Box<C>>) -> Self {
        let Some(source) = source else {
            return Self::none();
        };

        Self {
            data: Box::into_raw(source),
            allocator: Global,
        }
    }
}

impl<C, A: Allocator> CBox<C, A> {
    /// Set the pointer to null.
    pub const fn none() -> Self {
        Self {
            data: core::ptr::null_mut(),
            // SAFETY: allocator will never be used
            allocator: unsafe { core::mem::zeroed() },
        }
    }

    /// Returns `true` if the option is a `None` value.
    pub const fn is_none(&self) -> bool {
        self.data.is_null()
    }
}

#[cfg(feature = "alloc")]
impl<C: ReprC, A: Allocator> CBox<C, A> {
    /// Convert [`Self`] into [`Box<C>`]. Returns `None` if pointer is null.
    ///
    /// # Safety
    ///
    /// Check [`Box::from_raw`].
    pub unsafe fn into_rust(self) -> Option<Box<C>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { Box::from_raw(self.data) })
    }
}

#[cfg(feature = "alloc")]
impl<C> CBoxedSlice<C> {
    /// Create [`Self`] from a [`Box<[T]>`]
    pub fn from_boxed_slice(source: Option<Box<[C]>>) -> Self {
        let mut boxed_slice = core::mem::ManuallyDrop::new(source);

        let Some(boxed_slice) = boxed_slice.as_deref_mut() else {
            return Self::none();
        };

        Self {
            data: boxed_slice.as_mut_ptr(),
            len: boxed_slice.len(),
            allocator: Global,
        }
    }
}

impl<C, A: Allocator> CBoxedSlice<C, A> {
    /// Set the slice's data pointer to null
    pub const fn none() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len: 0,
            // SAFETY: allocator will never be used
            allocator: unsafe { core::mem::zeroed() },
        }
    }

    pub(crate) unsafe fn deallocate(&self) -> bool {
        if self.data.is_null() || self.len == 0 {
            return true;
        }

        if let Ok(layout) = core::alloc::Layout::array::<C>(self.len) {
            unsafe {
                self.allocator
                    .deallocate(NonNull::new_unchecked(self.data.cast()), layout);
            }

            return true;
        }

        false
    }
}

#[cfg(feature = "alloc")]
impl<C: ReprC, A: Allocator> CBoxedSlice<C, A> {
    /// Convert [`Self`] into a boxed slice. Return `None` if data pointer is null.
    /// Unlike [`Box::from_raw`], data pointer is allowed to be null.
    ///
    /// # Safety
    ///
    /// Check [`Box::from_raw`]
    pub unsafe fn into_rust(self) -> Option<Box<[C]>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(self.data, self.len)) })
    }
}

reprC! {
    unsafe impl(C, A: Allocator) Robust for CBox<C, A> {}
}

reprC! {
    unsafe impl(C, A: Allocator) Robust for CBoxedSlice<C, A> {}
}
