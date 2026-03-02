use alloc_crate::alloc::{self, alloc_zeroed, dealloc};
use core::{alloc::Layout, error::Error, num::NonZeroUsize, ptr::NonNull};

use crate::out_ptr::Zst;

// TODO: Use allocator-api2?

/// [`alloc_crate::alloc::Allocator`] but stateless
///
/// # Safety
///
/// Refer to [`alloc_crate::alloc::Allocator`]
// FIXME: Don't require Allocator be Zst + Copy?
pub unsafe trait Allocator: Zst + Copy {
    /// [`alloc_crate::alloc::Allocator::allocate`]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>;

    /// [`alloc_crate::alloc::Allocator::deallocate`]
    ///
    /// # Safety
    ///
    /// Refer to [`alloc_crate::alloc::Allocator::deallocate`]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);
}

/// [`alloc_crate::alloc::AllocError`]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct AllocError;

/// [`alloc_crate::alloc::Global`]
#[derive(Copy, Clone, Default, Debug)]
pub struct Global;

impl Error for AllocError {}
impl core::fmt::Display for AllocError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("memory allocation failed")
    }
}

impl Global {
    #[inline]
    fn alloc_impl(&self, layout: Layout, zeroed: bool) -> Result<NonNull<[u8]>, AllocError> {
        match layout.size() {
            0 => {
                let alignment = unsafe { NonZeroUsize::new_unchecked(layout.align()) };
                let dangling_layout = NonNull::without_provenance(alignment);
                Ok(NonNull::slice_from_raw_parts(dangling_layout, 0))
            }
            size => unsafe {
                let raw_ptr = if zeroed {
                    alloc_zeroed(layout)
                } else {
                    alloc::alloc(layout)
                };
                let ptr = NonNull::new(raw_ptr).ok_or(AllocError)?;
                Ok(NonNull::slice_from_raw_parts(ptr, size))
            },
        }
    }
}

unsafe impl Zst for Global {}
unsafe impl Allocator for Global {
    #[inline]
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.alloc_impl(layout, false)
    }

    #[inline]
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        if layout.size() != 0 {
            // SAFETY:
            // * We have checked that `layout` is non-zero in size.
            // * The caller is obligated to provide a layout that "fits", and in this case,
            //   "fit" always means a layout that is equal to the original, because our
            //   `allocate()`, `grow()`, and `shrink()` implementations never returns a larger
            //   allocation than requested.
            // * Other conditions must be upheld by the caller, as per `Allocator::deallocate()`'s
            //   safety documentation.
            unsafe { dealloc(ptr.as_ptr(), layout) }
        }
    }
}
