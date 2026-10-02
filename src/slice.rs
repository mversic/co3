//! C-ABI slice carriers.
use core::ptr::NonNull;
use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
    wide::Wide,
};

/// Fallibly converts a C-compatible representation into one ABI argument.
pub trait Unpack<Part>: ReprC<CType: Sized>
where
    Part: CType,
{
    /// Error returned when the ABI argument cannot be produced.
    type Error;

    /// Tries to convert the C representation into the ABI argument.
    fn unpack(value: Self::CType) -> Result<Part, Self::Error>;
}

/// Reconstructs a C-compatible representation from one ABI argument.
pub trait Pack<Part: CType>: ReprC<CType: Sized> {
    /// Error returned when the ABI argument cannot be converted.
    type Error;

    /// Tries to reconstruct the C representation.
    fn pack(part: Part) -> Result<Self::CType, Self::Error>;
}

impl<T, Part: CType + TryInto<Self::CType>> Pack<Part> for T
where
    Self: ReprC<CType: Sized>,
{
    type Error = <Part as TryInto<Self::CType>>::Error;

    #[inline(always)]
    fn pack(part: Part) -> Result<Self::CType, Self::Error> {
        part.try_into()
    }
}

impl<T, Part: CType + TryFrom<Self::CType>> Unpack<Part> for T
where
    Self: ReprC<CType: Sized>,
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
pub trait Unpack2<Part1: CType, Part2: CType>: ReprC<CType: Sized> {
    /// Error returned when either constituent cannot be produced.
    type Error;

    /// Tries to split the C representation into its constituents.
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error>;
}

/// Reconstructs a C-compatible representation from two ABI arguments.
pub trait Pack2<Part1: CType, Part2: CType>: ReprC<CType: Sized> {
    /// Error returned when the ABI arguments cannot be combined.
    type Error;

    /// Tries to reconstruct the C representation.
    fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error>;
}

impl<T: Pack2<Part1, Part2>, Part1: CType, Part2: CType> Pack2<Part1, Part2> for Option<T>
where
    Self: ReprC<CType = T::CType>,
{
    type Error = T::Error;

    #[inline(always)]
    fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
        T::pack(part1, part2)
    }
}

impl<T: Unpack2<Part1, Part2>, Part1: CType, Part2: CType> Unpack2<Part1, Part2> for Option<T>
where
    Self: ReprC<CType = T::CType>,
{
    type Error = T::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        T::unpack(value)
    }
}

macro_rules! impl_unpack2_for_transparent_wrapper {
    ($($wrapper:ty),+ $(,)?) => {$(
        impl<T: ?Sized, Part1: CType, Part2: CType> Unpack2<Part1, Part2> for $wrapper
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

macro_rules! impl_pack2_for_transparent_wrapper {
    ($($wrapper:ty),+ $(,)?) => {$(
        impl<T: ?Sized, Part1: CType, Part2: CType> Pack2<Part1, Part2> for $wrapper
        where
            T: Pack2<Part1, Part2>,
        {
            type Error = T::Error;

            #[inline(always)]
            fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
                T::pack(part1, part2)
            }
        }
    )+};
}

impl_unpack2_for_transparent_wrapper! {
    core::cell::UnsafeCell<T>,
    core::cell::Cell<T>,
    core::mem::ManuallyDrop<T>,
}

impl_pack2_for_transparent_wrapper! {
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
        unsafe impl<C: CType> BorrowCast for $ty<C> {
            type AsConst = Self;
        }
        unsafe impl<C: CType> BorrowCastMut for $ty<C> {
            type AsMut = Self;
        }
    };
}

impl_slice_carrier! { CSlice }
impl_slice_carrier! { CSliceMut }

impl<R: CType + Wide<Data = C, Metadata = usize> + ?Sized, C: CType, U: CType> Unpack2<*const C, U>
    for *const R
where
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*const C, U), Self::Error> {
        Ok((R::as_ptr(value), R::metadata(value).try_into()?))
    }
}

impl<R: CType + Wide<Data = C, Metadata = usize> + ?Sized, C: CType, U: CType> Unpack2<*mut C, U>
    for *mut R
where
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*mut C, U), Self::Error> {
        Ok((R::as_mut_ptr(value), R::metadata(value).try_into()?))
    }
}

impl<R: CType + Wide<Data = C, Metadata = usize> + ?Sized, C: CType, U: CType> Unpack2<*const C, U>
    for NonNull<R>
where
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*const C, U), Self::Error> {
        Ok((R::as_ptr(value), R::metadata(value).try_into()?))
    }
}

impl<R: CType + Wide<Data = C, Metadata = usize> + ?Sized, C: CType, U: CType> Unpack2<*mut C, U>
    for NonNull<R>
where
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*mut C, U), Self::Error> {
        Ok((R::as_mut_ptr(value), R::metadata(value).try_into()?))
    }
}

impl<R: ?Sized, C: CType, U: CType> Unpack2<*const C, U> for &R
where
    Self: ReprC<CType = CSlice<C>>,
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*const C, U), Self::Error> {
        Ok((value.data, value.len.try_into()?))
    }
}

impl<R: ?Sized, C: CType, U: CType + TryInto<usize>> Pack2<*const C, U> for &R
where
    Self: ReprC<CType = CSlice<C>>,
{
    type Error = <U as TryInto<usize>>::Error;

    #[inline(always)]
    fn pack(data: *const C, len: U) -> Result<Self::CType, Self::Error> {
        Ok(CSlice::from_raw_parts(data, len.try_into()?))
    }
}

impl<R: ?Sized, C: CType, U: CType> Unpack2<*mut C, U> for &R
where
    Self: ReprC<CType = CSliceMut<C>>,
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*mut C, U), Self::Error> {
        Ok((value.data, value.len.try_into()?))
    }
}

impl<R: ?Sized, C: CType, U: CType + TryInto<usize>> Pack2<*mut C, U> for &R
where
    Self: ReprC<CType = CSliceMut<C>>,
{
    type Error = <U as TryInto<usize>>::Error;

    #[inline(always)]
    fn pack(data: *mut C, len: U) -> Result<Self::CType, Self::Error> {
        Ok(CSliceMut::from_raw_parts_mut(data, len.try_into()?))
    }
}

impl<R: ?Sized, C: CType, U: CType> Unpack2<*mut C, U> for &mut R
where
    Self: ReprC<CType = CSliceMut<C>>,
    usize: TryInto<U>,
{
    type Error = <usize as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*mut C, U), Self::Error> {
        Ok((value.data, value.len.try_into()?))
    }
}

impl<R: ?Sized, C: CType, U: CType + TryInto<usize>> Pack2<*mut C, U> for &mut R
where
    Self: ReprC<CType = CSliceMut<C>>,
{
    type Error = <U as TryInto<usize>>::Error;

    #[inline(always)]
    fn pack(data: *mut C, len: U) -> Result<Self::CType, Self::Error> {
        Ok(CSliceMut::from_raw_parts_mut(data, len.try_into()?))
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
