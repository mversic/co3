//! C-ABI slice carriers.
use core::ptr::NonNull;
use rust_spec::RustSpec;

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    stored::{DecodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
    wide::{Wide, WideHeader},
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
    ///
    /// # Safety
    ///
    /// The parts must satisfy the ownership, validity, and exclusivity
    /// requirements of the representation being constructed.
    unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error>;
}

impl<T: Pack2<Part1, Part2>, Part1: CType, Part2: CType> Pack2<Part1, Part2> for Option<T>
where
    Self: ReprC<CType = T::CType>,
{
    type Error = T::Error;

    #[inline(always)]
    unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
        unsafe { T::pack(part1, part2) }
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

macro_rules! impl_pack2_for_transparent_wrapper {
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
        impl<T: ?Sized, Part1: CType, Part2: CType> Pack2<Part1, Part2> for $wrapper
        where
            T: Pack2<Part1, Part2>,
        {
            type Error = T::Error;

            #[inline(always)]
            unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
                unsafe { T::pack(part1, part2) }
            }
        }
    )+};
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

/// Mutable slice carrier with a defined C ABI layout.
///
/// `RESTRICTED = true` is the default and represents exclusive access; header
/// generators should render its data pointer as `C *restrict`. Use
/// `CSliceMut<C, false>` for a mutable pointer that may be aliased.
#[derive(RustSpec)]
#[repr(C)]
pub struct CSliceMut<C, const RESTRICTED: bool = true> {
    data: *mut C,
    len: usize,
}

macro_rules! impl_raw_slice_methods {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        impl<$($params)*> core::fmt::Debug for $ty<$($args)*> {
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
        impl<$($params)*> PartialEq for $ty<$($args)*> {
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
        impl<$($params)*> Eq for $ty<$($args)*> {}
        impl<$($params)*> PartialOrd for $ty<$($args)*> {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        impl<$($params)*> Ord for $ty<$($args)*> {
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
        impl<$($params)*> Clone for $ty<$($args)*> {
            fn clone(&self) -> Self {
                *self
            }
        }
        impl<$($params)*> Copy for $ty<$($args)*> {}
    };
}

impl_raw_slice_methods! { CSlice, [C], [C] }
impl_raw_slice_methods! { CSliceMut, [C, const RESTRICTED: bool], [C, RESTRICTED] }

impl<C> CSlice<C> {
    /// Set the slice's data pointer to null
    pub(crate) const NICHE: Self = Self {
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

impl<C, const RESTRICTED: bool> CSliceMut<C, RESTRICTED> {
    /// Create [`Self`] from mutable slice
    pub const fn from_slice(slice: &mut [C]) -> Self {
        Self {
            data: slice.as_mut_ptr(),
            len: slice.len(),
        }
    }

    /// Create a mutable slice carrier from raw parts.
    ///
    /// # Safety
    ///
    /// The pointer and length must describe a valid slice. When `RESTRICTED`
    /// is `true`, accesses must also satisfy exclusive access.
    pub const unsafe fn from_raw_parts_mut(data: *mut C, len: usize) -> Self {
        Self { data, len }
    }
}

macro_rules! impl_mut_slice_methods {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        impl<$($params)*> $ty<$($args)*> {
            pub(crate) const NICHE: Self = Self {
                data: core::ptr::null_mut(),
                len: 0,
            };

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
    };
}

impl_mut_slice_methods! { CSliceMut, [C, const RESTRICTED: bool], [C, RESTRICTED] }

macro_rules! impl_slice_carrier {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        unsafe impl<$($params)*> Borrow for $ty<$($args)*> {
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
        impl<'itm, $($params)*> FromBorrow<'itm> for $ty<$($args)*> {
            #[inline(always)]
            fn from_borrow(source: Self) -> Self {
                source
            }
        }

        impl<$($params)*> ReprC for $ty<$($args)*> where C: CType {
            type CType = Self;
        }
        unsafe impl<$($params)*> EncodeOwned for $ty<$($args)*> where C: CType {
            type Store = ();

            #[inline(always)]
            fn soft_encode<'itm>(self, (): &mut ()) -> Self::CType
            where
                Self: 'itm,
            {
                self
            }
        }
        unsafe impl<'d, $($params)*> DecodeOwned<'d> for $ty<$($args)*> where C: CType {
            type Store = ();

            #[inline(always)]
            unsafe fn soft_decode<'itm: 'd>(source: Self::CType, (): &mut ()) -> Option<Self> {
                Some(source)
            }
        }

        impl<$($params)*> Encode for $ty<$($args)*> where C: CType {}
        impl<'d, $($params)*> Decode<'d> for $ty<$($args)*> where C: CType {}

        unsafe impl<$($params)*> CheckedTransmute for $ty<$($args)*> where C: CType {
            #[inline(always)]
            unsafe fn is_valid(_: &Self::CType) -> bool {
                true
            }
        }

        unsafe impl<$($params)*> CType for $ty<$($args)*> where C: CType {}
        unsafe impl<$($params)*> CFnArg for $ty<$($args)*> where C: CType {}
        unsafe impl<$($params)*> CFnReturn for $ty<$($args)*> where C: CType {}
        unsafe impl<$($params)*> BorrowCast for $ty<$($args)*> where C: CType {
            type AsConst = Self;
        }
        unsafe impl<$($params)*> BorrowCastMut for $ty<$($args)*> where C: CType {
            type AsMut = Self;
        }
    };
}

impl_slice_carrier! { CSlice, [C], [C] }
impl_slice_carrier! { CSliceMut, [C, const RESTRICTED: bool], [C, RESTRICTED] }

macro_rules! impl_raw_wide_unpack {
    ($source:ty, $part:ty, $accessor:ident) => {
        impl<R: CType + ?Sized, C: CType, U: CType> Unpack2<$part, U> for $source
        where
            R: Wide<Header: WideHeader<Data = C>, Metadata = usize>,
            usize: TryInto<U>,
        {
            type Error = <usize as TryInto<U>>::Error;

            #[inline(always)]
            fn unpack(value: Self::CType) -> Result<($part, U), Self::Error> {
                Ok((R::$accessor(value).cast(), R::metadata(value).try_into()?))
            }
        }
    };
}

impl_raw_wide_unpack!(*const R, *const C, as_ptr);
impl_raw_wide_unpack!(*mut R, *mut C, as_mut_ptr);
impl_raw_wide_unpack!(NonNull<R>, *const C, as_ptr);
impl_raw_wide_unpack!(NonNull<R>, *mut C, as_mut_ptr);

macro_rules! construct_slice_carrier {
    (safe, $carrier:ty, $constructor:ident, $data:ident, $len:ident) => {
        <$carrier>::$constructor($data, $len)
    };
    (unsafe, $carrier:ty, $constructor:ident, $data:ident, $len:ident) => {
        unsafe { <$carrier>::$constructor($data, $len) }
    };
}

macro_rules! impl_ref_slice_pack {
    ($reference:ty, $carrier:ty, $pointer:ty, $constructor:ident, $safety:ident) => {
        impl<R: ?Sized, C: CType, U: CType> Unpack2<$pointer, U> for $reference
        where
            Self: ReprC<CType = $carrier>,
            usize: TryInto<U>,
        {
            type Error = <usize as TryInto<U>>::Error;

            #[inline(always)]
            fn unpack(value: Self::CType) -> Result<($pointer, U), Self::Error> {
                Ok((value.data, value.len.try_into()?))
            }
        }

        impl<R: ?Sized, C: CType, U: CType + TryInto<usize>> Pack2<$pointer, U> for $reference
        where
            Self: ReprC<CType = $carrier>,
        {
            type Error = <U as TryInto<usize>>::Error;

            #[inline(always)]
            unsafe fn pack(data: $pointer, len: U) -> Result<Self::CType, Self::Error> {
                let len = len.try_into()?;
                Ok(construct_slice_carrier!(
                    $safety,
                    $carrier,
                    $constructor,
                    data,
                    len
                ))
            }
        }
    };
}

impl_ref_slice_pack!(&R, CSlice<C>, *const C, from_raw_parts, safe);
impl_ref_slice_pack!(&R, CSliceMut<C, false>, *mut C, from_raw_parts_mut, unsafe);
impl_ref_slice_pack!(&mut R, CSliceMut<C>, *mut C, from_raw_parts_mut, unsafe);

#[cfg(feature = "alloc")]
impl<C> CSlice<C> {
    /// Convert into a shared slice. Return `None` if the data pointer is null.
    ///
    /// # Safety
    ///
    /// Check [`core::slice::from_raw_parts`].
    pub(crate) const unsafe fn into_rust<'slice>(self) -> Option<&'slice [C]> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { core::slice::from_raw_parts(self.data, self.len) })
    }
}

#[cfg(feature = "alloc")]
impl<C> CSliceMut<C> {
    /// Convert into an exclusive mutable slice. Return `None` if the data pointer is null.
    ///
    /// # Safety
    ///
    /// Check [`core::slice::from_raw_parts_mut`].
    pub(crate) const unsafe fn into_rust<'slice>(self) -> Option<&'slice mut [C]> {
        if self.data.is_null() {
            return None;
        }

        Some(unsafe { core::slice::from_raw_parts_mut(self.data, self.len) })
    }
}
