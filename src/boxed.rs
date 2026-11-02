//! Owning C-ABI carrier types.
pub use alloc::boxed::Box;
use core::ptr::NonNull;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, Zeroable,
    reference::impl_thin_pointer_traits,
    slice::{impl_carrier_borrow, impl_identity_carrier, impl_wide_raw_methods},
    stored::{DecodeMove, EncodeMove, EncodeMoveUnchecked},
    transmute::AbiCompatible,
    wide::{PointeeData, Wide, WideHeader},
};

pub(crate) trait FromBox<C: CType>: CType + Copy {
    fn data(self) -> *mut C;

    unsafe fn into_box(self) -> Option<Box<C>> {
        let ptr = NonNull::new(self.data())?;
        Some(unsafe { Box::from_raw(ptr.as_ptr()) })
    }
    unsafe fn from_non_null(data: NonNull<C>) -> Self;
}

impl<R: ReprC<CType: CType + Sized> + ?Sized> FromBox<R::CType> for CBox<R> {
    fn data(self) -> *mut R::CType {
        self.as_ptr()
    }

    unsafe fn from_non_null(data: NonNull<R::CType>) -> Self {
        unsafe { Self::from_non_null(data) }
    }
}

pub(crate) trait FromWideBox<P: Wide<Header: WideHeader<Data: CType>> + ?Sized>:
    CType + Sized
{
    unsafe fn from_raw_parts(data: NonNull<PointeeData<P>>, len: P::Metadata) -> Self;
    /// Builds a shared-view header. The caller must keep the `len` elements
    /// alive while the view is used; ownership is not transferred.
    fn data(&self) -> *mut PointeeData<P>;
    fn metadata(&self) -> P::Metadata;
}

impl<R, P> FromWideBox<P> for CWideBox<R>
where
    R: ReprC<CType = P> + ?Sized,
    P: Wide<Header: WideHeader<Data: CType>> + ?Sized,
{
    fn data(&self) -> *mut PointeeData<P> {
        Self::as_ptr(*self)
    }

    fn metadata(&self) -> P::Metadata {
        self.metadata
    }

    unsafe fn from_raw_parts(data: NonNull<PointeeData<P>>, len: P::Metadata) -> Self {
        unsafe { Self::from_raw_parts(data, len) }
    }
}

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

/// Owned wide pointee with a defined C ABI layout.
///
/// If the data pointer is set to `null`, the struct represents an absent box.
#[derive(RustSpec)]
#[repr(C)]
pub struct CWideBox<R: ReprC<CType: Wide> + ?Sized> {
    data: *mut PointeeData<R::CType>,
    metadata: <R::CType as Wide>::Metadata,
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

impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> Default for CWideBox<R> {
    fn default() -> Self {
        Self::null()
    }
}

impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> CWideBox<R> {
    /// Construct an absent carrier with initialized metadata.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
            metadata: unsafe { core::mem::zeroed() },
        }
    }
    /// Create a carrier from owned raw parts.
    ///
    /// # Safety
    ///
    /// The parts must retain unique ownership of a live allocation compatible with `P`.
    pub const unsafe fn from_raw_parts(
        data: NonNull<PointeeData<R::CType>>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self {
        Self {
            data: data.as_ptr(),
            metadata,
        }
    }
    /// Create a carrier from a boxed value.
    pub fn from_box(source: Box<R::CType>) -> Self {
        let metadata = R::CType::metadata(&*source);
        let data = R::CType::into_non_null(source).cast();
        Self {
            data: data.as_ptr(),
            metadata,
        }
    }
    /// Recover the owned value, or `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The parts must retain unique ownership of a live allocation compatible with `P`.
    pub unsafe fn into_box(self) -> Option<Box<R::CType>> {
        let data = NonNull::<<R::CType as Wide>::Header>::new(self.data.cast())?;
        Some(unsafe { R::CType::from_non_null(data, self.metadata) })
    }

    pub(crate) const unsafe fn from_raw_nullable(
        data: *mut PointeeData<R::CType>,
        metadata: <R::CType as Wide>::Metadata,
    ) -> Self {
        Self { data, metadata }
    }
}

impl<R: ReprC<CType: Wide> + ?Sized> CWideBox<R> {
    pub const fn as_ptr(self) -> *mut PointeeData<R::CType> {
        self.data
    }

    pub const fn metadata(&self) -> <R::CType as Wide>::Metadata {
        self.metadata
    }
}

impl_thin_pointer_traits! { CBox, [R: ReprC<CType: Sized> + ?Sized], [R] }
impl_wide_raw_methods! { CWideBox, [R: ReprC<CType: Wide> + ?Sized], [R] }

impl_identity_carrier! { CBox, [R: ReprC<CType: CType + Sized> + ?Sized], [R] }
impl_identity_carrier! { CWideBox, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized], [R] }

impl_carrier_borrow! { CBox, [R: ReprC<CType: CType + Sized> + ?Sized], [R] }
impl_carrier_borrow! { CWideBox, [R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized], [R] }

unsafe impl<R: ReprC<CType: Sized> + ?Sized> Zeroable for CBox<R> {}
unsafe impl<R: ReprC<CType: Wide> + ?Sized> Zeroable for CWideBox<R> {}

macro_rules! impl_vec_box_carrier {
    ($boxed:ident) => {
        impl<R: ReprC<CType: CType + Sized>> OwnedBoxCarrier<$boxed<[R]>> for crate::vec::CVec<R> {
            unsafe fn from_box_carrier(value: $boxed<[R]>) -> Option<Self> {
                let data = NonNull::new(value.as_ptr())?;
                let slice = core::ptr::slice_from_raw_parts_mut(data.as_ptr(), value.metadata());
                // SAFETY: The caller transfers ownership of the boxed allocation.
                Some(Self::from_vec(unsafe { Box::from_raw(slice) }.into_vec()))
            }
            unsafe fn into_box_carrier(self) -> $boxed<[R]> {
                // Shrink before discarding capacity, so Box deallocates with the correct layout.
                let value = unsafe { self.into_vec().expect("encoded owned vector is valid") };
                $boxed::from_box(value.into_boxed_slice())
            }
        }
    };
}
impl_vec_box_carrier!(CWideBox);
