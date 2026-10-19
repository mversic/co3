//! C-ABI borrowed wide pointer carriers.
use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
    ptr::NonNull,
};
use disjoint_impls::disjoint_impls;
use rust_spec::{RustSpec, drop::NoDrop};

use crate::{
    CFnArg, CFnReturn, CType, Decode, Encode, ReprC, WithDrop,
    borrow::{Borrow, BorrowCast, BorrowCastMut, FromBorrow},
    stored::{DecodeOwned, DirectEncodeOwned, EncodeOwned},
    transmute::CheckedTransmute,
    wide::{PointeeData, Wide, WideHeader},
};

/// Fallibly converts a C-compatible representation into one ABI argument.
pub trait Unpack<Part: CType>: ReprC<CType: Sized> {
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

/// Shared reference to a C-compatible wide pointee.
/// A null data pointer represents an absent reference.
#[derive(RustSpec)]
#[repr(C)]
pub struct CWideRef<P: Wide + ?Sized> {
    data: *const PointeeData<P>,
    len: usize,
}

/// Mutable slice carrier with a defined C ABI layout.
///
/// - `EXCLUSIVE = true` represents exclusive access that comes from `&mut T`.
/// - `EXCLUSIVE = false` represents access that comes from `&UnsafeCell<T>`.
///
/// `EXCLUSIVE == false` should be read as unknown. For all is known to this wrapper the access
/// is exclusive. You have to use `UnsafeCell` in `P` to assert non-exclusive access. It is the
/// user's responsibility to ensure any access of the pointer is well synchronized.
#[derive(RustSpec)]
#[repr(C)]
pub struct CWideRefMut<P: Wide + ?Sized, const EXCLUSIVE: bool> {
    data: *mut PointeeData<P>,
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

impl_raw_slice_methods! { CWideRef, [P: Wide + ?Sized], [P] }
impl_raw_slice_methods! { CWideRefMut, [P: Wide + ?Sized, const EXCLUSIVE: bool], [P, EXCLUSIVE] }

impl<P: Wide + ?Sized> CWideRef<P> {
    /// Construct a carrier with a null data pointer and zero length.
    pub const fn null() -> Self {
        Self {
            data: core::ptr::null(),
            len: 0,
        }
    }

    /// Return the stored data pointer.
    pub const fn as_ptr(self) -> *const PointeeData<P> {
        self.data
    }

    pub(crate) const fn data(&self) -> *const PointeeData<P> {
        self.data
    }

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

    pub(crate) const fn is_null(&self) -> bool {
        self.data.is_null()
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>> + ?Sized> CWideRef<P> {
    /// Create [`Self`] from a raw data pointer and slice metadata.
    ///
    /// # Safety
    ///
    /// A non-null pointer and length must describe a valid initialized slice
    /// for any access through the resulting carrier.
    pub const unsafe fn from_raw_parts(data: *const PointeeData<P>, len: usize) -> Self {
        Self { data, len }
    }
}

impl<P: Wide + ?Sized, const EXCLUSIVE: bool> CWideRefMut<P, EXCLUSIVE> {
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
}

impl<P: Wide<Header: WideHeader<Data: CType>> + ?Sized, const EXCLUSIVE: bool>
    CWideRefMut<P, EXCLUSIVE>
{
    /// Create a mutable slice carrier from raw parts.
    ///
    /// # Safety
    ///
    /// The pointer and length must describe a valid slice. When `EXCLUSIVE`
    /// is `true`, accesses must also satisfy exclusive access.
    pub const unsafe fn from_raw_parts_mut(data: *mut PointeeData<P>, len: usize) -> Self {
        Self { data, len }
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata = usize> + ?Sized> CWideRef<P> {
    /// Wrap an existing wide reference.
    pub fn from_ref(source: &P) -> Self {
        let len = P::metadata(source);
        let data = P::as_ptr(source).cast();
        Self { data, len }
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata = usize> + ?Sized, const EXCLUSIVE: bool>
    CWideRefMut<P, EXCLUSIVE>
{
    /// Wrap an existing mutable wide reference.
    pub fn from_mut(source: &mut P) -> Self {
        let len = P::metadata(source);
        let data = P::as_mut_ptr(source).cast();
        Self { data, len }
    }
}

macro_rules! impl_mut_slice_methods {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        impl<$($params)*> $ty<$($args)*> {
            pub(crate) const fn data(&self) -> *mut PointeeData<P> {
                self.data
            }

            #[expect(clippy::len_without_is_empty, reason = "nullable carrier uses Option<bool>")]
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

            pub(crate) const fn is_null(&self) -> bool {
                self.data.is_null()
            }
        }
    };
}

impl_mut_slice_methods! { CWideRefMut, [P: Wide + ?Sized, const EXCLUSIVE: bool], [P, EXCLUSIVE] }

macro_rules! impl_slice_carrier {
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
        unsafe impl<$($params)*> BorrowCast for $ty<$($args)*> {
            type AsConst = Self;
        }
        unsafe impl<$($params)*> BorrowCastMut for $ty<$($args)*> {
            type AsMut = Self;
        }
    };
}

macro_rules! impl_wide_slice_borrow {
    ($ty:ident, [$($params:tt)*], [$($args:tt)*]) => {
        unsafe impl<$($params)*> Borrow for $ty<$($args)*> {
            type Borrowed<'itm> = Self where Self: 'itm;
            type Owner = ();

            fn borrow<'itm>(self, (): &mut ()) -> Self::Borrowed<'itm>
            where
                Self: 'itm,
            {
                self
            }
        }

        impl<'itm, $($params)*> FromBorrow<'itm> for $ty<$($args)*> {
            fn from_borrow(source: Self) -> Self {
                source
            }
        }
    };
}

impl_wide_slice_borrow! { CWideRef, [P: Wide + ?Sized], [P] }
impl_wide_slice_borrow! { CWideRefMut, [P: Wide + ?Sized, const EXCLUSIVE: bool], [P, EXCLUSIVE] }

impl_slice_carrier! { CWideRef, [P: Wide<Header: WideHeader<Data: CType>> + ?Sized], [P] }
impl_slice_carrier! { CWideRefMut, [P: Wide<Header: WideHeader<Data: CType>> + ?Sized, const EXCLUSIVE: bool], [P, EXCLUSIVE] }

macro_rules! impl_raw_wide_unpack {
    ($source:ty, $part:ty, $accessor:ident) => {
        impl<R: CType + ?Sized, C: CType, U: CType> Unpack2<$part, U> for $source
        where
            R: Wide<Header: WideHeader<Data = C>> + Wide<Metadata = usize>,
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
    (unsafe, $carrier:ty, $constructor:ident, $data:ident, $len:ident) => {
        unsafe { <$carrier>::$constructor($data, $len) }
    };
}

macro_rules! impl_ref_slice_pack {
    ($reference:ty, $carrier:ty, $pointer:ty, $constructor:ident, $safety:ident) => {
        impl<R: ?Sized, P: Wide<Header: WideHeader<Data = C>> + ?Sized, C: CType, U: CType>
            Unpack2<$pointer, U> for $reference
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

        impl<
            R: ?Sized,
            P: Wide<Header: WideHeader<Data = C>> + ?Sized,
            C: CType,
            U: CType + TryInto<usize>,
        > Pack2<$pointer, U> for $reference
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

impl_ref_slice_pack!(&R, CWideRef<P>, *const C, from_raw_parts, unsafe);
impl_ref_slice_pack!(&R, CWideRefMut<P, false>, *mut C, from_raw_parts_mut, unsafe);
impl_ref_slice_pack!(&mut R, CWideRefMut<P, true>, *mut C, from_raw_parts_mut, unsafe);

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata = usize> + ?Sized> CWideRef<P> {
    /// Recover a shared wide reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer and metadata must describe a valid `P` for the returned lifetime.
    pub unsafe fn into_ref<'a>(self) -> Option<&'a P> {
        if self.data.is_null() {
            return None;
        }

        let len = self.len;
        let data = self.data.cast();

        Some(unsafe { P::from_raw_parts(data, len) })
    }
}

impl<P: Wide<Header: WideHeader<Data: CType>, Metadata = usize> + ?Sized> CWideRefMut<P, true> {
    /// Recover a mutable wide reference, returning `None` for a null pointer.
    ///
    /// # Safety
    ///
    /// The pointer and metadata must describe a valid, exclusively accessible `P`
    /// for the returned lifetime.
    pub unsafe fn into_mut<'a>(self) -> Option<&'a mut P> {
        if self.data.is_null() {
            return None;
        }

        let len = self.len;
        let data = self.data.cast();

        Some(unsafe { P::from_raw_parts_mut(data, len) })
    }
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

disjoint_impls! {
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

    impl<T: Unpack2<Part1, Part2>, Part1: CType, Part2: CType> Unpack2<Part1, Part2> for ManuallyDrop<T>
    where
        T: RustSpec<Drop = NoDrop>,
    {
        type Error = T::Error;

        #[inline(always)]
        fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
            T::unpack(value)
        }
    }

    impl<T: Unpack2<Part1, Part2>, Part1: CType, Part2: CType> Unpack2<Part1, Part2> for ManuallyDrop<T>
    where
        T: RustSpec<Drop: WithDrop>,
    {
        type Error = T::Error;

        #[inline(always)]
        fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
            T::unpack(ManuallyDrop::into_inner(value))
        }
    }
}

disjoint_impls! {
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

    impl<T: Pack2<Part1, Part2>, Part1: CType, Part2: CType> Pack2<Part1, Part2> for ManuallyDrop<T>
    where
        T: RustSpec<Drop = NoDrop>,
    {
        type Error = T::Error;

        #[inline(always)]
        unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
            unsafe { T::pack(part1, part2) }
        }
    }

    impl<T: Pack2<Part1, Part2>, Part1: CType, Part2: CType> Pack2<Part1, Part2> for ManuallyDrop<T>
    where
        T: RustSpec<Drop: WithDrop>,
    {
        type Error = T::Error;

        #[inline(always)]
        unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
            unsafe { T::pack(part1, part2) }.map(ManuallyDrop::new)
        }
    }
}

impl<T: Unpack2<Part1, Part2>, Part1: CType, Part2: CType> Unpack2<Part1, Part2> for UnsafeCell<T> {
    type Error = T::Error;

    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        T::unpack(value.into_inner())
    }
}

impl<T: Pack2<Part1, Part2>, Part1: CType, Part2: CType> Pack2<Part1, Part2> for UnsafeCell<T> {
    type Error = T::Error;

    unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
        unsafe { T::pack(part1, part2) }.map(UnsafeCell::new)
    }
}

impl<T: Unpack2<Part1, Part2>, Part1: CType, Part2: CType> Unpack2<Part1, Part2> for Cell<T> {
    type Error = T::Error;

    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        T::unpack(value.into_inner())
    }
}

impl<T: Pack2<Part1, Part2>, Part1: CType, Part2: CType> Pack2<Part1, Part2> for Cell<T> {
    type Error = T::Error;

    unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
        unsafe { T::pack(part1, part2) }.map(UnsafeCell::new)
    }
}

#[cfg(feature = "alloc")]
pub(crate) use impl_raw_slice_methods;
