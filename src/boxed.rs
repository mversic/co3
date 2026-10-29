//! Owning C-ABI carrier types.
pub use alloc::boxed::Box;
use core::ptr::NonNull;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{BorrowCast, BorrowCastMut},
    reference::{CRef, CRefMut},
    slice::{CWideRef, CWideRefMut, impl_identity_carrier, impl_wide_raw_methods},
    stored::{DecodeOwned, EncodeMove, EncodeMoveUnchecked},
    transmute::AbiCompatible,
    wide::{PointeeData, Wide, WideHeader},
};

pub(crate) trait FromBox<C: CType>: BorrowCastMut<AsMut = CRefMut<C, true>> + Copy {
    unsafe fn into_box(self) -> Option<Box<C>> {
        let ptr = NonNull::new(crate::borrow::borrow_cast_mut(self).as_ptr())?;
        Some(unsafe { Box::from_raw(ptr.as_ptr()) })
    }
    unsafe fn from_non_null(data: NonNull<C>) -> Self;
}

impl<C: CType> FromBox<C> for CBox<C> {
    unsafe fn from_non_null(data: NonNull<C>) -> Self {
        unsafe { CBox::from_non_null(data) }
    }
}

impl<C: CType> FromBox<C> for CBoxCell<C> {
    unsafe fn from_non_null(data: NonNull<C>) -> Self {
        unsafe { CBoxCell::from_non_null(data) }
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

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> FromWideBox<P>
    for CWideBoxCell<P>
{
    fn data(&self) -> *mut PointeeData<P> {
        CWideBoxCell::as_ptr(*self)
    }

    fn metadata(&self) -> P::Metadata {
        self.metadata
    }

    unsafe fn from_raw_parts(data: NonNull<PointeeData<P>>, len: P::Metadata) -> Self {
        unsafe { CWideBoxCell::from_raw_parts(data, len) }
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

/// Owned pointer whose shared borrowed view permits interior mutation.
///
/// Like [`CBox`], a null data pointer represents `Option<Box<C>>`.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CBoxCell<C> {
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

/// Owned slice whose shared borrowed view permits interior mutation.
///
/// Like [`CWideBox`], a null data pointer represents `Option<Box<[C]>>`.
#[derive(RustSpec)]
#[repr(C)]
pub struct CWideBoxCell<P: Wide + ?Sized> {
    data: *mut PointeeData<P>,
    metadata: P::Metadata,
}

macro_rules! impl_boxed_pointer {
    ($ty:ident, $as_const:ty) => {
        impl<C> core::fmt::Debug for $ty<C> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_struct(stringify!($ty))
                    .field("data", &self.data)
                    .finish_non_exhaustive()
            }
        }

        impl<C> Eq for $ty<C> {}
        impl<C> PartialEq for $ty<C> {
            fn eq(&self, other: &Self) -> bool {
                self.data == other.data
            }
        }

        impl<C> Ord for $ty<C> {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.data.cmp(&other.data)
            }
        }
        impl<C> PartialOrd for $ty<C> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        impl<C> Copy for $ty<C> {}
        impl<C> Clone for $ty<C> {
            fn clone(&self) -> Self {
                *self
            }
        }

        impl<C: CType> Default for $ty<C> {
            fn default() -> Self {
                Self::null()
            }
        }

        impl<C: CType> $ty<C> {
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

            // TODO: This method should not be
            pub(crate) const fn from_raw(data: *mut C) -> Self {
                Self { data }
            }
        }

        impl<C> $ty<C> {
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

        unsafe impl<C: CType> BorrowCast for $ty<C> {
            type AsConst = $as_const;
        }
        unsafe impl<C: CType> BorrowCastMut for $ty<C> {
            type AsMut = CRefMut<C, true>;
        }
    };
}

macro_rules! impl_wide_box_methods {
    ($ty:ident) => {
        impl_wide_raw_methods! { $ty, [P: Wide + ?Sized], [P] }

        impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> Default
            for $ty<P>
        {
            fn default() -> Self {
                Self::null()
            }
        }

        impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> $ty<P> {
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

            pub(crate) const fn from_raw_nullable(
                data: *mut PointeeData<P>,
                metadata: P::Metadata,
            ) -> Self {
                Self { data, metadata }
            }
        }

        impl<P: Wide + ?Sized> $ty<P> {
            pub const fn as_ptr(self) -> *mut PointeeData<P> {
                self.data
            }

            pub const fn metadata(&self) -> P::Metadata {
                self.metadata
            }
        }
    };
}

impl_boxed_pointer!(CBox, CRef<C>);
impl_boxed_pointer!(CBoxCell, CRefMut<C, false>);

impl_wide_box_methods!(CWideBox);
impl_wide_box_methods!(CWideBoxCell);

impl_identity_carrier! { CBox, [C: CType], [C] }
impl_identity_carrier! { CBoxCell, [C: CType], [C] }
impl_identity_carrier! { CWideBox, [P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized], [P] }
impl_identity_carrier! { CWideBoxCell, [P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized], [P] }

unsafe impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> BorrowCast
    for CWideBox<P>
{
    type AsConst = CWideRef<P>;
}
unsafe impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> BorrowCastMut
    for CWideBox<P>
{
    type AsMut = CWideRefMut<P, true>;
}

unsafe impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> BorrowCast
    for CWideBoxCell<P>
{
    type AsConst = CWideRefMut<P, false>;
}
unsafe impl<P: Wide<Header: WideHeader<Data: CType>, Metadata: CType> + ?Sized> BorrowCastMut
    for CWideBoxCell<P>
{
    type AsMut = CWideRefMut<P, true>;
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
macro_rules! impl_vec_box_carrier {
    ($boxed:ident) => {
        impl<T: CType> OwnedBoxCarrier<$boxed<[T]>> for crate::vec::CVec<T> {
            unsafe fn from_box_carrier(value: $boxed<[T]>) -> Option<Self> {
                let data = NonNull::new(value.as_ptr())?;
                let slice = core::ptr::slice_from_raw_parts_mut(data.as_ptr(), value.metadata());
                // SAFETY: The caller transfers ownership of the boxed allocation.
                Some(Self::from_vec(unsafe { Box::from_raw(slice) }.into_vec()))
            }
            unsafe fn into_box_carrier(self) -> $boxed<[T]> {
                // Shrink before discarding capacity, so Box deallocates with the correct layout.
                let value = unsafe { self.into_vec().expect("encoded owned vector is valid") };
                $boxed::from_box(value.into_boxed_slice())
            }
        }
    };
}
impl_vec_box_carrier!(CWideBox);
impl_vec_box_carrier!(CWideBoxCell);

/// Constructs a borrowed mutable view while retaining ownership in the original carrier.
pub(crate) trait OwnedMutView: Copy {
    type View: CType + Copy;
    fn mut_view(self) -> Self::View;
}
impl<C: BorrowCastMut<AsMut: Copy> + Copy> OwnedMutView for C {
    type View = C::AsMut;
    fn mut_view(self) -> Self::View {
        crate::borrow::borrow_cast_mut(self)
    }
}
