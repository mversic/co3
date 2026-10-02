//! Owning C-ABI carrier types.
pub use alloc::boxed::Box;
use core::ptr::NonNull;

use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{BorrowCast, BorrowCastMut},
    slice::{CSlice, CSliceMut, Pack2, Unpack2},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

/// Owned pointer derived from `Box<C>`.
///
/// If the data pointer is set to `null`, the struct represents `Option<Box<C>>`.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CBox<C> {
    pub(crate) data: *mut C,
}

/// Owned pointer whose shared borrowed view permits interior mutation.
///
/// Like [`CBox`], a null data pointer represents `Option<Box<C>>`.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CBoxCell<C> {
    pub(crate) data: *mut C,
}

macro_rules! impl_boxed_pointer {
    ($ty:ident, $as_const:tt) => {
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
            /// Create [`Self`] from a [`Box<C>`].
            pub fn from_box(source: Box<C>) -> Self {
                Self { data: Box::into_raw(source) }
            }

            pub(crate) const NICHE_VALUE: Self = Self { data: core::ptr::null_mut() };

            /// Recover the allocation, returning `None` for the null niche.
            ///
            /// # Safety
            ///
            /// Check [`Box::from_raw`].
            pub(crate) unsafe fn into_rust(self) -> Option<Box<C>> {
                if self.data.is_null() {
                    return None;
                }
                Some(unsafe { Box::from_raw(self.data) })
            }

            #[allow(unused)]
            pub(crate) const fn is_niche(&self) -> bool {
                self.data.is_null()
            }
        }

        unsafe impl<C: CType> BorrowCast for $ty<C> {
            type AsConst = *$as_const C;
        }
        unsafe impl<C: CType> BorrowCastMut for $ty<C> {
            type AsMut = *mut C;
        }
    };
}

impl_boxed_pointer!(CBox, const);
impl_boxed_pointer!(CBoxCell, mut);

/// Owned slice `Box<[C]>` with a defined C ABI layout. Consists of a data pointer and a length.
/// Used in place of a function out-pointer to transfer ownership of the slice to the caller.
/// If the data pointer is set to `null`, the struct represents `Option<Box<[C]>>`.
#[derive(RustSpec)]
#[repr(C)]
pub struct CBoxedSlice<C> {
    pub(crate) data: *mut C,
    len: usize,
}

/// Owned slice whose shared borrowed view permits interior mutation.
#[derive(RustSpec)]
#[repr(transparent)]
pub struct CBoxedSliceCell<C>(CBoxedSlice<C>);

impl<C> core::fmt::Debug for CBoxedSliceCell<C> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}
impl<C> PartialEq for CBoxedSliceCell<C> {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
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
        self.0.cmp(&other.0)
    }
}
impl<C> Clone for CBoxedSliceCell<C> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<C> Copy for CBoxedSliceCell<C> {}

impl<C> CBoxedSliceCell<C> {
    pub fn from_boxed_slice(source: Box<[C]>) -> Self {
        Self(CBoxedSlice::from_boxed_slice(source))
    }

    /// # Safety
    ///
    /// Check [`Box::from_raw`].
    pub(crate) unsafe fn into_rust(self) -> Option<Box<[C]>> {
        unsafe { self.0.into_rust() }
    }

    pub(crate) const NICHE_VALUE: Self = Self(CBoxedSlice::NICHE_VALUE);
}

impl<C> core::fmt::Debug for CBoxedSlice<C> {
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

impl<C> PartialEq for CBoxedSlice<C> {
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

impl<C> Eq for CBoxedSlice<C> {}

impl<C> PartialOrd for CBoxedSlice<C> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<C> Ord for CBoxedSlice<C> {
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

impl<C> Clone for CBoxedSlice<C> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C> Copy for CBoxedSlice<C> {}

impl<C> CBox<C> {
    /// Create [`Self`] from a raw data pointer
    pub(crate) const fn from_raw_parts(data: NonNull<C>) -> Self {
        Self {
            data: data.as_ptr(),
        }
    }
}

impl<C> CBoxedSlice<C> {
    /// Create [`Self`] from a [`Box<[T]>`]
    pub fn from_boxed_slice(source: Box<[C]>) -> Self {
        let len = source.len();

        Self {
            data: Box::into_raw(source).cast(),
            len,
        }
    }

    /// Create [`Self`] from a raw data pointer and slice metadata.
    pub(crate) const fn from_raw_parts(data: NonNull<C>, len: usize) -> Self {
        Self {
            data: data.as_ptr(),
            len,
        }
    }

    /// Convert [`Self`] into [`Box<[C]>`]. Returns `None` if pointer is null.
    ///
    /// # Safety
    ///
    /// Check [`Box::from_raw`].
    pub(crate) unsafe fn into_rust(self) -> Option<Box<[C]>> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(self.data, self.len)) })
    }
}

impl<C> CBoxedSlice<C> {
    /// Set the slice's data pointer to null
    pub(crate) const NICHE_VALUE: Self = Self {
        data: core::ptr::null_mut(),
        len: 0,
    };

    pub(crate) const fn is_niche(&self) -> bool {
        self.data.is_null()
    }

    pub(crate) const fn data(&self) -> *mut C {
        self.data
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
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
    type AsMut = CSliceMut<C>;
}

unsafe impl<C: CType> BorrowCast for CBoxedSliceCell<C> {
    type AsConst = CSliceMut<C>;
}
unsafe impl<C: CType> BorrowCastMut for CBoxedSliceCell<C> {
    type AsMut = CSliceMut<C>;
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
    fn pack(data: K, len: U) -> Result<Self::CType, Self::Error> {
        let data: CBox<C> = data.into();
        Ok(CBoxedSlice {
            data: data.data,
            len: len.try_into()?,
        })
    }
}
