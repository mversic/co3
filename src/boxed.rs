//! Owning C-ABI carrier types.
pub use alloc::boxed::Box;
use core::ptr::NonNull;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{BorrowCast, BorrowCastMut},
    reference::{CRef, CRefMut},
    slice::{CWideRef, CWideRefMut, Pack2, Unpack2, impl_raw_slice_methods},
    stored::{DecodeOwned, DirectEncodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
    wide::{PointeeData, Wide, WideHeader},
};

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
    len: usize,
}

/// Owned slice whose shared borrowed view permits interior mutation.
///
/// Like [`CWideBox`], a null data pointer represents `Option<Box<[C]>>`.
#[derive(RustSpec)]
#[repr(C)]
pub struct CWideBoxCell<P: Wide + ?Sized> {
    data: *mut PointeeData<P>,
    len: usize,
}

macro_rules! impl_boxed_pointer {
    ($ty:ident, $as_const:ty) => {
        impl<C: CType> core::fmt::Debug for $ty<C> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_struct(stringify!($ty))
                    .field("data", &self.data)
                    .finish_non_exhaustive()
            }
        }
        impl<C: CType> PartialEq for $ty<C> {
            fn eq(&self, other: &Self) -> bool {
                self.data == other.data
            }
        }
        impl<C: CType> Eq for $ty<C> {}
        impl<C: CType> PartialOrd for $ty<C> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl<C: CType> Ord for $ty<C> {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.data.cmp(&other.data)
            }
        }
        impl<C: CType> Clone for $ty<C> {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<C: CType> Copy for $ty<C> {}

        impl<C> $ty<C> {
            /// Construct a carrier with a null data pointer.
            pub const fn null() -> Self {
                Self {
                    data: core::ptr::null_mut(),
                }
            }

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

        impl<C: CType> $ty<C> {
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
        }

        unsafe impl<C: CType> BorrowCast for $ty<C> {
            type AsConst = $as_const;
        }
        unsafe impl<C: CType> BorrowCastMut for $ty<C> {
            type AsMut = CRefMut<C, true>;
        }
    };
}

impl_boxed_pointer!(CBox, CRef<C>);
impl_boxed_pointer!(CBoxCell, CRefMut<C, false>);

impl<P: Wide + ?Sized> core::fmt::Debug for CWideBoxCell<P> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CWideBoxCell")
            .field("data", &self.data)
            .field("len", &self.len)
            .finish()
    }
}
impl<P: Wide + ?Sized> PartialEq for CWideBoxCell<P> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data && self.len == other.len
    }
}
impl<P: Wide + ?Sized> Eq for CWideBoxCell<P> {}
impl<P: Wide + ?Sized> PartialOrd for CWideBoxCell<P> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<P: Wide + ?Sized> Ord for CWideBoxCell<P> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.data, self.len).cmp(&(other.data, other.len))
    }
}
impl<P: Wide + ?Sized> Clone for CWideBoxCell<P> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<P: Wide + ?Sized> Copy for CWideBoxCell<P> {}

impl<P: Wide + ?Sized> CWideBoxCell<P> {
    /// Construct a carrier with a null data pointer and zero length.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len: 0,
        }
    }

    /// Return the stored data pointer.
    pub const fn as_ptr(self) -> *mut PointeeData<P> {
        self.data
    }

    /// Return the number of elements in the slice.
    #[expect(
        clippy::len_without_is_empty,
        reason = "nullable carrier uses Option<bool>"
    )]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Return `None` for a null carrier, or whether the present slice is empty.
    pub const fn is_empty(&self) -> Option<bool> {
        if self.data.is_null() {
            None
        } else {
            Some(self.len == 0)
        }
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>> + ?Sized> CWideBoxCell<P> {
    /// Create an owning slice carrier from a non-null pointer and length.
    ///
    /// # Safety
    ///
    /// `data` and `len` must describe a uniquely owned live allocation compatible
    /// with the boxed slice later recovered from this carrier.
    pub const unsafe fn from_raw_parts(data: NonNull<PointeeData<P>>, len: usize) -> Self {
        Self {
            data: data.as_ptr(),
            len,
        }
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata = usize> + ?Sized> CWideBoxCell<P> {
    /// Create [`Self`] from a [`Box<P>`].
    pub fn from_box(source: Box<P>) -> Self {
        let len = P::metadata(&*source);
        let data = P::into_non_null(source).cast();
        Self {
            data: data.as_ptr(),
            len,
        }
    }

    /// Recover the owned wide value, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer and metadata must retain unique ownership of the allocation
    /// and describe a valid `P` value.
    pub unsafe fn into_box(self) -> Option<Box<P>> {
        let len = self.len;
        let data = NonNull::<P::Header>::new(self.data.cast())?;
        Some(unsafe { P::from_non_null(data, len) })
    }
}

impl_raw_slice_methods! { CWideBox, [P: Wide + ?Sized], [P] }

impl<P: Wide + ?Sized> CWideBox<P> {
    /// Construct a carrier with a null data pointer and zero length.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len: 0,
        }
    }

    /// Return the stored data pointer.
    pub const fn as_ptr(self) -> *mut PointeeData<P> {
        self.data
    }

    /// Return the number of elements in the slice.
    #[expect(
        clippy::len_without_is_empty,
        reason = "nullable carrier uses Option<bool>"
    )]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Return `None` for a null carrier, or whether the present slice is empty.
    pub const fn is_empty(&self) -> Option<bool> {
        if self.data.is_null() {
            None
        } else {
            Some(self.len == 0)
        }
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>> + ?Sized> CWideBox<P> {
    /// Create [`Self`] from a non-null pointer and length.
    ///
    /// # Safety
    ///
    /// `data` and `len` must retain unique ownership and provenance of a live
    /// allocation compatible with the owning slice later recovered from this carrier.
    pub const unsafe fn from_raw_parts(data: NonNull<PointeeData<P>>, len: usize) -> Self {
        Self {
            data: data.as_ptr(),
            len,
        }
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata = usize> + ?Sized> CWideBox<P> {
    /// Create [`Self`] from a [`Box<P>`].
    pub fn from_box(source: Box<P>) -> Self {
        let len = P::metadata(&*source);
        let data = P::into_non_null(source).cast();
        Self {
            data: data.as_ptr(),
            len,
        }
    }

    /// Recover the owned wide value, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer and metadata must retain unique ownership of the allocation
    /// and describe a valid `P` value.
    pub unsafe fn into_box(self) -> Option<Box<P>> {
        let len = self.len;
        let data = NonNull::<P::Header>::new(self.data.cast())?;
        Some(unsafe { P::from_non_null(data, len) })
    }
}

impl<P: Wide + ?Sized> CWideBox<P> {
    pub(crate) const fn is_null(&self) -> bool {
        self.data.is_null()
    }

    pub(crate) const fn data(&self) -> *mut PointeeData<P> {
        self.data
    }
}

macro_rules! impl_boxed_carrier {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        impl<$($params)*> ReprC for $ty<$($args)*> {
            type CType = Self;
        }
        unsafe impl<$($params)*> EncodeOwned for $ty<$($args)*> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                unsafe { self.soft_encode_unchecked(&mut ()) }
            }
        }
        unsafe impl<$($params)*> DirectEncodeOwned for $ty<$($args)*> {
            type SoftStore = ();

            unsafe fn soft_encode_unchecked(self, (): &mut ()) -> Self::CType {
                self
            }
        }
        unsafe impl<'d, $($params)*> DecodeOwned<'d> for $ty<$($args)*> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl<$($params)*> Encode for $ty<$($args)*> {}
        impl<$($params)*> crate::DirectEncode for $ty<$($args)*> {}
        impl<'d, $($params)*> Decode<'d> for $ty<$($args)*> {}

        unsafe impl<$($params)*> CheckedTransmute for $ty<$($args)*> {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<$($params)*> CType for $ty<$($args)*> {}
        unsafe impl<$($params)*> CFnArg for $ty<$($args)*> {}
        unsafe impl<$($params)*> CFnReturn for $ty<$($args)*> {}
    };
}

impl_boxed_carrier! { CBox, [C: CType], [C] }
impl_boxed_carrier! { CBoxCell, [C: CType], [C] }
impl_boxed_carrier! { CWideBox, [P: Wide<Header: WideHeader<Data: CType>> + ?Sized], [P] }
impl_boxed_carrier! { CWideBoxCell, [P: Wide<Header: WideHeader<Data: CType>> + ?Sized], [P] }

unsafe impl<P: Wide<Header: WideHeader<Data: CType>> + ?Sized> BorrowCast for CWideBox<P> {
    type AsConst = CWideRef<P>;
}
unsafe impl<P: Wide<Header: WideHeader<Data: CType>> + ?Sized> BorrowCastMut for CWideBox<P> {
    type AsMut = CWideRefMut<P, true>;
}

unsafe impl<P: Wide<Header: WideHeader<Data: CType>> + ?Sized> BorrowCast for CWideBoxCell<P> {
    type AsConst = CWideRefMut<P, false>;
}
unsafe impl<P: Wide<Header: WideHeader<Data: CType>> + ?Sized> BorrowCastMut for CWideBoxCell<P> {
    type AsMut = CWideRefMut<P, true>;
}

impl<R: ?Sized, P: Wide<Header: WideHeader<Data: CType>> + ?Sized, K: CType, U: CType> Unpack2<K, U>
    for Box<R>
where
    Self: ReprC<CType = CWideBox<P>>,
    CBox<PointeeData<P>>: Into<K>,
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(K, U), Self::Error> {
        Ok((CBox { data: value.data }.into(), value.len.try_into()?))
    }
}

impl<R: ?Sized, P: Wide<Header: WideHeader<Data: CType>> + ?Sized, K: CType, U: CType> Pack2<K, U>
    for Box<R>
where
    Self: ReprC<CType = CWideBox<P>>,
    K: Into<CBox<PointeeData<P>>>,
    U: TryInto<usize>,
{
    type Error = <U as TryInto<usize>>::Error;

    #[inline(always)]
    unsafe fn pack(data: K, len: U) -> Result<Self::CType, Self::Error> {
        let data: CBox<PointeeData<P>> = data.into();
        Ok(CWideBox {
            data: data.data,
            len: len.try_into()?,
        })
    }
}
