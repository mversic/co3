//! Owning C-ABI carrier types.
pub use alloc::boxed::Box;
use core::ptr::NonNull;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{BorrowCast, BorrowCastMut},
    reference::{CRef, CRefMut},
    slice::{CSlice, CSliceMut, Pack2, Unpack2, impl_raw_slice_methods},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
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

/// Owned slice `Box<[C]>` with a defined C ABI layout.
///
/// If the data pointer is set to `null`, the struct represents `Option<Box<[C]>>`.
#[derive(RustSpec)]
#[repr(C)]
pub struct CBoxedSlice<C> {
    data: *mut C,
    len: usize,
}

/// Owned slice whose shared borrowed view permits interior mutation.
///
/// Like [`CBoxedSlice`], a null data pointer represents `Option<Box<[C]>>`.
#[derive(RustSpec)]
#[repr(C)]
pub struct CBoxedSliceCell<C> {
    data: *mut C,
    len: usize,
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
        impl<C> PartialEq for $ty<C> {
            fn eq(&self, other: &Self) -> bool {
                self.data == other.data
            }
        }
        impl<C> Eq for $ty<C> {}
        impl<C> PartialOrd for $ty<C> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl<C> Ord for $ty<C> {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.data.cmp(&other.data)
            }
        }
        impl<C> Clone for $ty<C> {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<C> Copy for $ty<C> {}

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

            /// Create [`Self`] from a [`Box<C>`].
            pub fn from_box(source: Box<C>) -> Self {
                Self {
                    data: Box::into_raw(source),
                }
            }

            /// Recover the allocation, returning `None` for a null pointer.
            ///
            /// # Safety
            ///
            /// A non-null pointer must uniquely own a live allocation compatible with `Box<C>`.
            pub unsafe fn into_rust(self) -> Option<Box<C>> {
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

impl<C> core::fmt::Debug for CBoxedSliceCell<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CBoxedSliceCell")
            .field("data", &self.data)
            .field("len", &self.len)
            .finish()
    }
}
impl<C> PartialEq for CBoxedSliceCell<C> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data && self.len == other.len
    }
}
impl<C> Eq for CBoxedSliceCell<C> {}
impl<C> PartialOrd for CBoxedSliceCell<C> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<C> Ord for CBoxedSliceCell<C> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        (self.data, self.len).cmp(&(other.data, other.len))
    }
}
impl<C> Clone for CBoxedSliceCell<C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C> Copy for CBoxedSliceCell<C> {}

impl<C> CBoxedSliceCell<C> {
    /// Construct a carrier with a null data pointer and zero length.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len: 0,
        }
    }

    pub fn from_boxed_slice(source: Box<[C]>) -> Self {
        let len = source.len();

        Self {
            data: Box::into_raw(source).cast(),
            len,
        }
    }

    /// Return the stored data pointer.
    pub const fn as_ptr(self) -> *mut C {
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

    /// Create an owning slice carrier from a non-null pointer and length.
    ///
    /// # Safety
    ///
    /// `data` and `len` must describe a uniquely owned live allocation compatible
    /// with the boxed slice later recovered from this carrier.
    pub const unsafe fn from_raw_parts(data: NonNull<C>, len: usize) -> Self {
        Self {
            data: data.as_ptr(),
            len,
        }
    }

    /// Recover the allocation, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// A non-null pointer and length must uniquely own a live allocation compatible with `Box<[C]>`.
    pub unsafe fn into_rust(self) -> Option<Box<[C]>> {
        if self.data.is_null() {
            None
        } else {
            Some(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(self.data, self.len)) })
        }
    }
}

impl_raw_slice_methods! { CBoxedSlice, [C], [C] }

impl<C> CBoxedSlice<C> {
    /// Construct a carrier with a null data pointer and zero length.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len: 0,
        }
    }

    /// Create [`Self`] from a [`Box<[T]>`]
    pub fn from_boxed_slice(source: Box<[C]>) -> Self {
        let len = source.len();

        Self {
            data: Box::into_raw(source).cast(),
            len,
        }
    }

    /// Return the stored data pointer.
    pub const fn as_ptr(self) -> *mut C {
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

    /// Create [`Self`] from a non-null pointer and length.
    ///
    /// # Safety
    ///
    /// `data` and `len` must retain unique ownership and provenance of a live
    /// allocation compatible with the owning slice later recovered from this carrier.
    pub const unsafe fn from_raw_parts(data: NonNull<C>, len: usize) -> Self {
        Self {
            data: data.as_ptr(),
            len,
        }
    }

    /// Recover the allocation, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// A non-null pointer and length must uniquely own a live allocation compatible with `Box<[C]>`.
    pub unsafe fn into_rust(self) -> Option<Box<[C]>> {
        if self.data.is_null() {
            None
        } else {
            Some(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(self.data, self.len)) })
        }
    }
}

impl<C> CBoxedSlice<C> {
    pub(crate) const fn is_null(&self) -> bool {
        self.data.is_null()
    }

    pub(crate) const fn data(&self) -> *mut C {
        self.data
    }
}

macro_rules! impl_boxed_carrier {
    ($ty:ident) => {
        impl<C: CType> ReprC for $ty<C> {
            type CType = Self;
        }
        unsafe impl<C: CType> EncodeOwned for $ty<C> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d, C: CType> DecodeOwned<'d> for $ty<C> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl<C: CType> Encode for $ty<C> {}
        impl<'d, C: CType> Decode<'d> for $ty<C> {}

        unsafe impl<C: CType> CheckedTransmute for $ty<C> {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<C: CType> CType for $ty<C> {}
        unsafe impl<C: CType> CFnArg for $ty<C> {}
        unsafe impl<C: CType> CFnReturn for $ty<C> {}
    };
}

impl_boxed_carrier! { CBox }
impl_boxed_carrier! { CBoxCell }
impl_boxed_carrier! { CBoxedSlice }
impl_boxed_carrier! { CBoxedSliceCell }

unsafe impl<C: CType> BorrowCast for CBoxedSlice<C> {
    type AsConst = CSlice<C>;
}
unsafe impl<C: CType> BorrowCastMut for CBoxedSlice<C> {
    type AsMut = CSliceMut<C, true>;
}

unsafe impl<C: CType> BorrowCast for CBoxedSliceCell<C> {
    type AsConst = CSliceMut<C, false>;
}
unsafe impl<C: CType> BorrowCastMut for CBoxedSliceCell<C> {
    type AsMut = CSliceMut<C, true>;
}

impl<R: ?Sized, C: CType, K: CType, U: CType> Unpack2<K, U> for Box<R>
where
    Self: ReprC<CType = CBoxedSlice<C>>,
    CBox<C>: Into<K>,
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(K, U), Self::Error> {
        Ok((CBox { data: value.data }.into(), value.len.try_into()?))
    }
}

impl<R: ?Sized, C: CType, K: CType + Into<CBox<C>>, U: CType + TryInto<usize>> Pack2<K, U>
    for Box<R>
where
    Self: ReprC<CType = CBoxedSlice<C>>,
{
    type Error = <U as TryInto<usize>>::Error;

    #[inline(always)]
    unsafe fn pack(data: K, len: U) -> Result<Self::CType, Self::Error> {
        let data: CBox<C> = data.into();
        Ok(CBoxedSlice {
            data: data.data,
            len: len.try_into()?,
        })
    }
}
