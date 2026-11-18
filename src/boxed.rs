//! Owning C-ABI carrier types.
pub use alloc::boxed::Box as RustBox;
use core::ptr::NonNull;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, ReprC, Zeroable,
    convert::{Decode, DecodeMove, Encode, EncodeMove, EncodeMoveUnchecked},
    reference::impl_thin_pointer_traits,
    transmute::AbiCompatible,
    wide::{CWideBox, impl_carrier_borrow, impl_identity_carrier},
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

/// Owned ABI pointer to a lowered `T: CType` pointee.
///
/// If the data pointer is set to `null`, the struct represents `Option<RustBox<T>>`.
#[derive(RustSpec)]
#[repr(transparent)]
#[doc(hidden)]
pub struct Box<T: CType> {
    data: *mut T,
}

impl<T: CType> core::fmt::Debug for Box<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Box")
            .field("data", &self.data)
            .finish_non_exhaustive()
    }
}

impl<T: CType> Default for Box<T> {
    fn default() -> Self {
        Self::null()
    }
}

impl<T: CType> Box<T> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
        }
    }

    /// Create [`Self`] from a box containing the lowered pointee.
    pub fn from_box(source: RustBox<T>) -> Self {
        Self {
            data: RustBox::into_raw(source),
        }
    }

    /// Create [`Self`] from a raw owned pointer.
    ///
    /// # Safety
    ///
    /// `data` must retain unique ownership and provenance of a live allocation
    /// compatible with the owning type later recovered from this carrier.
    pub const unsafe fn from_non_null(data: NonNull<T>) -> Self {
        Self {
            data: data.as_ptr(),
        }
    }

    pub(crate) const unsafe fn from_raw(data: *mut T) -> Self {
        Self { data }
    }
}

impl<T: CType> Box<T> {
    /// Return the stored data pointer.
    pub const fn as_ptr(self) -> *mut T {
        self.data
    }

    /// Recover the allocation, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// A non-null pointer must uniquely own a live allocation compatible with `RustBox<T>`.
    pub unsafe fn into_box(self) -> Option<RustBox<T>> {
        if self.data.is_null() {
            None
        } else {
            Some(unsafe { RustBox::from_raw(self.data) })
        }
    }

    #[allow(unused)]
    pub(crate) const fn is_null(&self) -> bool {
        self.data.is_null()
    }
}

impl_thin_pointer_traits! { Box, [T: CType], [T] }
impl_identity_carrier! { Box, [T: CType], [T] }
impl_carrier_borrow! { Box, [T: CType], [T] }

impl<R: ReprC<CType: CType + Sized>> OwnedBoxCarrier<CWideBox<[R]>> for crate::vec::CVec<R> {
    unsafe fn from_box_carrier(value: CWideBox<[R]>) -> Option<Self> {
        let data = NonNull::new(value.as_ptr())?;
        let slice = core::ptr::slice_from_raw_parts_mut(data.as_ptr(), value.metadata());
        // SAFETY: The caller transfers ownership of the boxed allocation.
        Some(Self::from_vec(
            unsafe { RustBox::from_raw(slice) }.into_vec(),
        ))
    }

    unsafe fn into_box_carrier(self) -> CWideBox<[R]> {
        // Shrink before discarding capacity, so Box deallocates with the correct layout.
        let value = unsafe { self.into_vec().expect("encoded owned vector is valid") };
        CWideBox::from_box(value.into_boxed_slice())
    }
}

unsafe impl<T: CType> Zeroable for Box<T> {}

#[cfg(test)]
mod tests {
    use super::{Box, RustBox};
    use crate::{CType, ReprC, primitives::Bool};
    use static_assertions::{assert_impl_all, assert_type_eq_all};

    struct Payload;
    unsafe impl CType for Payload {}

    #[test]
    fn carrier_accepts_lowered_payload_without_source_mapping() {
        assert_impl_all!(Box<Payload>: CType, ReprC);
        assert_type_eq_all!(<Box<Payload> as ReprC>::CType, Box<Payload>);
        assert_type_eq_all!(<RustBox<bool> as ReprC>::CType, Box<Bool>);

        let source = RustBox::new(Payload);
        let pointer = core::ptr::from_ref(&*source);
        let carrier = Box::from_box(source);
        assert_eq!(carrier.as_ptr().cast_const(), pointer);
        let recovered = unsafe { carrier.into_box() }.unwrap();
        assert_eq!(core::ptr::from_ref(&*recovered), pointer);
    }
}
