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

impl<C: CType> FromBox<C> for CBox<C> {
    fn data(self) -> *mut C {
        self.as_ptr()
    }

    unsafe fn from_non_null(data: NonNull<C>) -> Self {
        unsafe { CBox::from_non_null(data) }
    }
}

pub(crate) trait FromWideBox<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized>:
    CType + Sized
{
    unsafe fn from_raw_parts(data: NonNull<PointeeData<P>>, len: P::Metadata) -> Self;
    /// Builds a shared-view header. The caller must keep the `len` elements
    /// alive while the view is used; ownership is not transferred.
    fn data(&self) -> *mut PointeeData<P>;
    fn metadata(&self) -> P::Metadata;
}

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> FromWideBox<P>
    for CWideBox<P>
{
    fn data(&self) -> *mut PointeeData<P> {
        CWideBox::as_ptr(*self)
    }

    fn metadata(&self) -> P::Metadata {
        self.metadata
    }

    unsafe fn from_raw_parts(data: NonNull<PointeeData<P>>, len: P::Metadata) -> Self {
        unsafe { CWideBox::from_raw_parts(data, len) }
    }
}

/// Converts owned carrier allocations without assuming their headers have equal layout.
pub(crate) trait OwnedBoxCarrier<B>: Sized {
    unsafe fn from_box_carrier(value: B) -> Option<Self>;
    unsafe fn into_box_carrier(self) -> B;
}
impl<C> OwnedBoxCarrier<C> for C {
    unsafe fn from_box_carrier(value: C) -> Option<Self> {
        Some(value)
    }
    unsafe fn into_box_carrier(self) -> C {
        self
    }
}

/// Owned pointer derived from `Box<C>`.
///
/// If the data pointer is set to `null`, the struct represents `Option<Box<C>>`.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CBox<C> {
    data: *mut C,
}

/// Owned wide pointee with a defined C ABI layout.
///
/// If the data pointer is set to `null`, the struct represents `Option<Box<[C]>>`.
#[derive(RustSpec)]
#[repr(C)]
pub struct CWideBox<P: Wide + ?Sized> {
    data: *mut PointeeData<P>,
    metadata: P::Metadata,
}

impl<C> core::fmt::Debug for CBox<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CBox")
            .field("data", &self.data)
            .finish_non_exhaustive()
    }
}

impl<C: CType> Default for CBox<C> {
    fn default() -> Self {
        Self::null()
    }
}

impl<C: CType> CBox<C> {
    /// Construct a carrier with a null data pointer.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
        }
    }

    /// Create [`Self`] from a [`Box<C>`].
    pub fn from_box(source: Box<C>) -> Self {
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
    pub const unsafe fn from_non_null(data: NonNull<C>) -> Self {
        Self {
            data: data.as_ptr(),
        }
    }

    pub(crate) const unsafe fn from_raw(data: *mut C) -> Self {
        Self { data }
    }
}

impl<C> CBox<C> {
    /// Return the stored data pointer.
    pub const fn as_ptr(self) -> *mut C {
        self.data
    }

    /// Recover the allocation, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// A non-null pointer must uniquely own a live allocation compatible with `Box<C>`.
    pub unsafe fn into_box(self) -> Option<Box<C>> {
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

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> Default for CWideBox<P> {
    fn default() -> Self {
        Self::null()
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> CWideBox<P> {
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
        data: NonNull<PointeeData<P>>,
        metadata: P::Metadata,
    ) -> Self {
        Self {
            data: data.as_ptr(),
            metadata,
        }
    }
    /// Create a carrier from a boxed value.
    pub fn from_box(source: Box<P>) -> Self {
        let metadata = P::metadata(&*source);
        let data = P::into_non_null(source).cast();
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
    pub unsafe fn into_box(self) -> Option<Box<P>> {
        let data = NonNull::<P::Header>::new(self.data.cast())?;
        Some(unsafe { P::from_non_null(data, self.metadata) })
    }

    pub(crate) const unsafe fn from_raw_nullable(
        data: *mut PointeeData<P>,
        metadata: P::Metadata,
    ) -> Self {
        Self { data, metadata }
    }
}

impl<P: Wide + ?Sized> CWideBox<P> {
    pub const fn as_ptr(self) -> *mut PointeeData<P> {
        self.data
    }

    pub const fn metadata(&self) -> P::Metadata {
        self.metadata
    }
}

impl_thin_pointer_traits! { CBox, [C], [C] }
impl_wide_raw_methods! { CWideBox, [P: Wide + ?Sized], [P] }

impl_identity_carrier! { CBox, [C: CType], [C] }
impl_identity_carrier! { CWideBox, [P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized], [P] }

impl_carrier_borrow! { CBox, [C: CType], [C] }
impl_carrier_borrow! { CWideBox, [P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized], [P] }

unsafe impl<C> Zeroable for CBox<C> {}
unsafe impl<P: Wide + ?Sized> Zeroable for CWideBox<P> {}

macro_rules! impl_vec_box_carrier {
    ($boxed:ident) => {
        impl<R: ReprC<CType: CType + Sized>> OwnedBoxCarrier<$boxed<[R::CType]>>
            for crate::vec::CVec<R>
        {
            unsafe fn from_box_carrier(value: $boxed<[R::CType]>) -> Option<Self> {
                let data = NonNull::new(value.as_ptr())?;
                let slice = core::ptr::slice_from_raw_parts_mut(data.as_ptr(), value.metadata());
                // SAFETY: The caller transfers ownership of the boxed allocation.
                Some(Self::from_vec(unsafe { Box::from_raw(slice) }.into_vec()))
            }
            unsafe fn into_box_carrier(self) -> $boxed<[R::CType]> {
                // Shrink before discarding capacity, so Box deallocates with the correct layout.
                let value = unsafe { self.into_vec().expect("encoded owned vector is valid") };
                $boxed::from_box(value.into_boxed_slice())
            }
        }
    };
}
impl_vec_box_carrier!(CWideBox);
