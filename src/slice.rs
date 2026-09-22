//! C-ABI slice carriers.
use rust_spec::RustSpec;

use crate::{
    CFnArg, Decode, Encode, ExternC, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
};

/// Fallibly converts a C-compatible representation into one ABI argument.
pub trait Unpack<Part>: ExternC<CType: Sized>
where
    Part: ReprC,
{
    /// Error returned when the ABI argument cannot be produced.
    type Error;

    /// Tries to convert the C representation into the ABI argument.
    fn unpack(value: Self::CType) -> Result<Part, Self::Error>;
}

impl<T, Part: ReprC + TryFrom<Self::CType>> Unpack<Part> for T
where
    Self: ExternC<CType: Sized>,
{
    type Error = <Part as TryFrom<Self::CType>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<Part, Self::Error> {
        Part::try_from(value)
    }
}

/// Unpacks a C-compatible representation as two ABI arguments.
///
/// This is used by `#[unpack(T1, T2)]` in [`crate::ffi!`] declarations.
/// Fallibly unpacks a C-compatible representation as two ABI arguments.
pub trait Unpack2<Part1: ReprC, Part2: ReprC>: ExternC<CType: Sized> {
    /// Error returned when either constituent cannot be produced.
    type Error;

    /// Tries to split the C representation into its constituents.
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error>;
}

impl<T: Unpack2<Part1, Part2>, Part1: ReprC, Part2: ReprC> Unpack2<Part1, Part2> for Option<T>
where
    Self: ExternC<CType = T::CType>,
{
    type Error = T::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        T::unpack(value)
    }
}

macro_rules! impl_unpack2_for_transparent_wrapper {
    ($($wrapper:ty),+ $(,)?) => {$(
        impl<T: ?Sized, Part1: ReprC, Part2: ReprC> Unpack2<Part1, Part2> for $wrapper
        where
            T: Unpack2<Part1, Part2>,
        {
            type Error = T::Error;

            #[inline(always)]
            fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
                T::unpack(value)
            }
        }
    )+};
}

impl_unpack2_for_transparent_wrapper! {
    core::cell::UnsafeCell<T>,
    core::cell::Cell<T>,
    core::mem::ManuallyDrop<T>,
}

/// Immutable slice `&[C]` with a defined C ABI layout. Consists of a data pointer and a length.
/// If the data pointer is set to `null`, the struct represents `Option<&[C]>`.
#[derive(RustSpec)]
#[repr(C)]
pub struct CSlice<C> {
    data: *const C,
    len: usize,
}

/// Mutable slice `&mut [C]` with a defined C ABI layout. Consists of a data pointer and a length.
/// If the data pointer is set to `null`, the struct represents `Option<&mut [C]>`.
#[derive(RustSpec)]
#[repr(C)]
pub struct CSliceMut<C> {
    data: *mut C,
    len: usize,
}

macro_rules! impl_raw_slice_methods {
    ($($ty:ty),+ $(,)?) => {$(
        impl<C> core::fmt::Debug for $ty {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                if self.data.is_null() {
                    f.debug_struct(stringify!($ty))
                        .field("data", &self.data)
                        .finish_non_exhaustive()
                } else {
                    f.debug_struct(stringify!($ty))
                        .field("data", &self.data)
                        .field("len", &self.len)
                        .finish()
                }
            }
        }
        impl<C> PartialEq for $ty {
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
        impl<C> Eq for $ty {}
        impl<C> PartialOrd for $ty {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl<C> Ord for $ty {
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
        impl<C> Clone for $ty {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<C> Copy for $ty {})+
    };
}

impl_raw_slice_methods! { CSlice<C>, CSliceMut<C> }

impl<C> CSlice<C> {
    /// Set the slice's data pointer to null
    pub(crate) const NICHE_VALUE: Self = Self {
        data: core::ptr::null(),
        // TODO: Use MaybeUninit for len?
        len: 0,
    };

    /// Create [`Self`] from shared slice
    pub const fn from_slice(slice: &[C]) -> Self {
        Self {
            data: slice.as_ptr(),
            len: slice.len(),
        }
    }

    /// Create [`Self`] from a raw data pointer and slice metadata.
    ///
    /// Before decoding this carrier into a Rust reference, the caller must
    /// ensure the pointer and length describe a valid initialized slice.
    pub const fn from_raw_parts(data: *const C, len: usize) -> Self {
        Self { data, len }
    }

    pub(crate) const fn data(&self) -> *const C {
        self.data
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    pub(crate) const fn is_niche(&self) -> bool {
        self.data.is_null()
    }
}

impl<C> CSliceMut<C> {
    /// Set the slice's data pointer to null
    pub(crate) const NICHE_VALUE: Self = Self {
        data: core::ptr::null_mut(),
        // TODO: Use MaybeUninit for len?
        len: 0,
    };

    /// Create [`Self`] from mutable slice
    pub const fn from_slice(slice: &mut [C]) -> Self {
        Self {
            data: slice.as_mut_ptr(),
            len: slice.len(),
        }
    }

    /// Create [`Self`] from a raw data pointer and slice metadata.
    ///
    /// Before decoding this carrier into a Rust mutable reference, the caller
    /// must ensure the pointer and length describe a valid initialized slice
    /// with exclusive access.
    pub const fn from_raw_parts_mut(data: *mut C, len: usize) -> Self {
        Self { data, len }
    }

    pub(crate) const fn data(&self) -> *mut C {
        self.data
    }

    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    pub(crate) const fn is_niche(&self) -> bool {
        self.data.is_null()
    }
}

macro_rules! impl_slice_carrier {
    ($ty:ident) => {
        unsafe impl<C> Borrow for $ty<C> {
            type Borrowed<'itm>
                = Self
            where
                Self: 'itm;

            type Owner = ();

            #[inline(always)]
            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }
        impl<'itm, C> FromBorrow<'itm> for $ty<C> {
            #[inline(always)]
            fn from_borrow(source: Self) -> Self {
                source
            }
        }

        impl<C: ReprC> ExternC for $ty<C> {
            type CType = Self;
        }
        unsafe impl<C: ReprC> EncodeOwned for $ty<C> {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d, C: ReprC> DecodeOwned<'d> for $ty<C> {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl<C: ReprC> Encode for $ty<C> {}
        impl<'d, C: ReprC> Decode<'d> for $ty<C> {}

        unsafe impl<C: ReprC> CheckedTransmute for $ty<C> {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<C: ReprC> ReprC for $ty<C> {}
        unsafe impl<C: ReprC> CFnArg for $ty<C> {}
        unsafe impl<C: ReprC> BorrowCast for $ty<C> {
            type AsConst = Self;
        }
        unsafe impl<C: ReprC> BorrowCastMut for $ty<C> {
            type AsMut = Self;
        }
    };
}

impl_slice_carrier! { CSlice }
impl_slice_carrier! { CSliceMut }

impl<R: ?Sized, C: ReprC, U: ReprC> Unpack2<*const C, U> for &R
where
    Self: ExternC<CType = CSlice<C>>,
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*const C, U), Self::Error> {
        Ok((value.data, value.len.try_into()?))
    }
}

impl<R: ?Sized, C: ReprC, U: ReprC> Unpack2<*mut C, U> for &R
where
    Self: ExternC<CType = CSliceMut<C>>,
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*mut C, U), Self::Error> {
        Ok((value.data, value.len.try_into()?))
    }
}

impl<R: ?Sized, C: ReprC, U: ReprC> Unpack2<*mut C, U> for &mut R
where
    Self: ExternC<CType = CSliceMut<C>>,
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*mut C, U), Self::Error> {
        Ok((value.data, value.len.try_into()?))
    }
}

#[cfg(feature = "alloc")]
impl<C> CSlice<C> {
    /// Convert [`Self`] into a mutable slice. Return `None` if data pointer is null.
    /// Unlike [`core::slice::from_raw_parts_mut`], data pointer is allowed to be null.
    ///
    /// # Safety
    ///
    /// Check [`core::slice::from_raw_parts_mut`]
    pub(crate) const unsafe fn into_rust<'slice>(self) -> Option<&'slice [C]> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { core::slice::from_raw_parts(self.data, self.len) })
    }
}

#[cfg(feature = "alloc")]
impl<C> CSliceMut<C> {
    /// Convert [`Self`] into a mutable slice. Return `None` if data pointer is null.
    /// Unlike [`core::slice::from_raw_parts_mut`], data pointer is allowed to be null.
    ///
    /// # Safety
    ///
    /// Check [`core::slice::from_raw_parts_mut`]
    pub(crate) const unsafe fn into_rust<'slice>(self) -> Option<&'slice mut [C]> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { core::slice::from_raw_parts_mut(self.data, self.len) })
    }
}
