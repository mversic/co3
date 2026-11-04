//! Owning C-ABI carrier types.
pub use alloc::boxed::Box;
use core::ptr::NonNull;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, Zeroable,
    reference::impl_thin_pointer_traits,
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked},
    transmute::AbiCompatible,
    wide::CWideBox,
    wide::{impl_carrier_borrow, impl_identity_carrier},
};

/// Converts owned carrier allocations without assuming their headers have equal layout.
pub(crate) trait OwnedBoxCarrier<B>: Sized {
    unsafe fn from_box_carrier(value: B) -> Option<Self>;
    unsafe fn into_box_carrier(self) -> B;
}
impl<C> OwnedBoxCarrier<C> for C {
    unsafe fn from_box_carrier(value: Self) -> Option<Self> {
        Some(value)
    }
    unsafe fn into_box_carrier(self) -> Self {
        self
    }
}

/// Owned pointer carrying a lowered `Box<R>` pointee.
///
/// If the data pointer is set to `null`, the struct represents `Option<Box<R>>`.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CBox<R: ReprC<CType: Sized> + ?Sized> {
    data: *mut R::CType,
}

impl<R: ReprC<CType: Sized> + ?Sized> core::fmt::Debug for CBox<R> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CBox")
            .field("data", &self.data)
            .finish_non_exhaustive()
    }
}

impl<R: ReprC<CType: CType + Sized> + ?Sized> Default for CBox<R> {
    fn default() -> Self {
        Self::null()
    }
}

impl<R: ReprC<CType: CType + Sized> + ?Sized> CBox<R> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
        }
    }

    /// Create [`Self`] from a box containing the lowered pointee.
    pub fn from_box(source: Box<R::CType>) -> Self {
        Self {
            data: Box::into_raw(source),
        }
    }

    /// Create [`Self`] from a raw owned pointer.
    ///
    /// # Safety
    ///
    /// `data` must retain unique ownership and provenance of a live allocation
    /// compatible with the owning type later recovered from this carrier.
    pub const unsafe fn from_non_null(data: NonNull<R::CType>) -> Self {
        Self {
            data: data.as_ptr(),
        }
    }

    pub(crate) const unsafe fn from_raw(data: *mut R::CType) -> Self {
        Self { data }
    }
}

impl<R: ReprC<CType: Sized> + ?Sized> CBox<R> {
    /// Return the stored data pointer.
    pub const fn as_ptr(self) -> *mut R::CType {
        self.data
    }

    /// Recover the allocation, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// A non-null pointer must uniquely own a live allocation compatible with `Box<R::CType>`.
    pub unsafe fn into_box(self) -> Option<Box<R::CType>> {
        if self.data.is_null() {
            None
        } else {
            Some(unsafe { Box::from_raw(self.data) })
        }
    }

    #[allow(unused)]
    pub(crate) const fn is_null(&self) -> bool {
        self.data.is_null()
    }
}

impl_thin_pointer_traits! { CBox, [R: ReprC<CType: Sized> + ?Sized], [R] }
impl_identity_carrier! { CBox, [R: ReprC<CType: CType + Sized> + ?Sized], [R] }
impl_carrier_borrow! { CBox, [R: ReprC<CType: CType + Sized> + ?Sized], [R] }

impl<R: ReprC<CType: CType + Sized>> OwnedBoxCarrier<CWideBox<[R]>> for crate::vec::CVec<R> {
    unsafe fn from_box_carrier(value: CWideBox<[R]>) -> Option<Self> {
        let data = NonNull::new(value.as_ptr())?;
        let slice = core::ptr::slice_from_raw_parts_mut(data.as_ptr(), value.metadata());
        // SAFETY: The caller transfers ownership of the boxed allocation.
        Some(Self::from_vec(unsafe { Box::from_raw(slice) }.into_vec()))
    }

    unsafe fn into_box_carrier(self) -> CWideBox<[R]> {
        // Shrink before discarding capacity, so Box deallocates with the correct layout.
        let value = unsafe { self.into_vec().expect("encoded owned vector is valid") };
        CWideBox::from_box(value.into_boxed_slice())
    }
}

unsafe impl<R: ReprC<CType: Sized> + ?Sized> Zeroable for CBox<R> {}
