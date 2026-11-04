//! Conversion between C-compatible representations and ABI arguments.

use core::{
    cell::{Cell, UnsafeCell},
    mem::ManuallyDrop,
    ptr::NonNull,
};

use disjoint_impls::disjoint_impls;
use rust_spec::{RustSpec, drop::NoDrop};

use crate::{
    CType, NeedsDrop, ReprC,
    borrow::Borrowed,
    option::COptionSome,
    tuple::Tuple2,
    wide::{CSoftWideRefMut, CSoftWideRef},
    wide::{Wide, WideHeader},
};
#[cfg(feature = "alloc")]
use crate::{
    boxed::{Box, CBox},
    wide::CWideBox,
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

macro_rules! impl_raw_wide_unpack {
    (@unwrap $value:ident identity) => { $value };
    (@unwrap $value:ident into_inner) => { $value.0 };
    ($source:ty, *$pointer:ident, $accessor:ident, $unwrap:ident) => {
        impl<R: Wide<Header: WideHeader<Data: CType>, Metadata: TryInto<U>> + CType + ?Sized, U: CType>
            Unpack2<*$pointer <R::Header as WideHeader>::Data, U> for $source
        {
            type Error = <R::Metadata as TryInto<U>>::Error;

            #[inline(always)]
            fn unpack(value: Self::CType) -> Result<(*$pointer <R::Header as WideHeader>::Data, U), Self::Error> {
                let value = impl_raw_wide_unpack!(@unwrap value $unwrap);
                Ok((R::$accessor(value).cast(), R::metadata(value).try_into()?))
            }
        }
        // TODO: Implement Pack2
    };
}

impl_raw_wide_unpack!(*const R, *const, as_ptr, identity);
impl_raw_wide_unpack!(*mut R, *mut, as_mut_ptr, identity);
impl_raw_wide_unpack!(NonNull<R>, *const, as_ptr, into_inner);
impl_raw_wide_unpack!(NonNull<R>, *mut, as_mut_ptr, into_inner);

macro_rules! impl_ref_wide_pack {
    ($reference:ty, $carrier:ty, *$pointer:ident, $constructor:ident) => {
        impl<R: ReprC<CType = P> + Wide + ?Sized, P: Wide<Header: WideHeader<Data: CType>, Metadata: TryInto<U>> + ?Sized, U: CType>
            Unpack2<*$pointer <P::Header as WideHeader>::Data, U> for $reference
        where
            Self: ReprC<CType = $carrier>,
        {
            type Error = <P::Metadata as TryInto<U>>::Error;

            #[inline(always)]
            fn unpack(value: Self::CType) -> Result<(*$pointer <P::Header as WideHeader>::Data, U), Self::Error> {
                Ok((value.as_ptr(), value.metadata().try_into()?))
            }
        }

        impl<R: ReprC<CType = P> + Wide + ?Sized, P: Wide<Header: WideHeader<Data: CType>> + ?Sized, U: CType + TryInto<P::Metadata>>
            Pack2<*$pointer <P::Header as WideHeader>::Data, U> for $reference
        where
            Self: ReprC<CType = $carrier>,
        {
            type Error = <U as TryInto<P::Metadata>>::Error;

            #[inline(always)]
            unsafe fn pack(data: *$pointer <P::Header as WideHeader>::Data, metadata: U) -> Result<Self::CType, Self::Error> {
                let metadata = metadata.try_into()?;
                Ok(unsafe { <$carrier>::$constructor(data, metadata) })
            }
        }
    };
}

impl_ref_wide_pack!(&R, CSoftWideRef<R>, *const, from_raw_parts);
impl_ref_wide_pack!(&R, CSoftWideRefMut<R, false>, *mut, from_raw_parts_mut);
impl_ref_wide_pack!(&mut R, CSoftWideRefMut<R, true>, *mut, from_raw_parts_mut);

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
    Self: ReprC<CType = COptionSome<T>>,
{
    type Error = T::Error;

    #[inline(always)]
    unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
        unsafe { T::pack(part1, part2) }.map(COptionSome::from_carrier)
    }
}

impl<T: Unpack2<Part1, Part2>, Part1: CType, Part2: CType> Unpack2<Part1, Part2> for Option<T>
where
    Self: ReprC<CType = COptionSome<T>>,
{
    type Error = T::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        T::unpack(value.into_carrier())
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
        T: RustSpec<Drop: NeedsDrop>,
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
        T: RustSpec<Drop: NeedsDrop>,
    {
        type Error = T::Error;

        #[inline(always)]
        unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
            unsafe { T::pack(part1, part2) }.map(ManuallyDrop::new)
        }
    }
}

#[cfg(feature = "alloc")]
impl<
    R: ReprC<CType = P> + Wide + ?Sized,
    P: Wide<Header: WideHeader<Data: CType + Sized>, Metadata: TryInto<U>> + ?Sized,
    K: CType,
    U: CType,
> Unpack2<K, U> for Box<R>
where
    Self: ReprC<CType = CWideBox<R>>,
    <<R as Wide>::Header as WideHeader>::Data:
        ReprC<CType = <<P as Wide>::Header as WideHeader>::Data>,
    CBox<<<R as Wide>::Header as WideHeader>::Data>: Into<K>,
{
    type Error = <<P as Wide>::Metadata as TryInto<U>>::Error;
    fn unpack(value: Self::CType) -> Result<(K, U), Self::Error> {
        Ok((
            unsafe { CBox::from_raw(value.as_ptr()) }.into(),
            value.metadata().try_into()?,
        ))
    }
}

#[cfg(feature = "alloc")]
impl<
    R: ReprC<CType = P> + Wide + ?Sized,
    P: Wide<Header: WideHeader<Data: CType + Sized>> + ?Sized,
    K: CType,
    U: CType + TryInto<<P as Wide>::Metadata>,
> Pack2<K, U> for Box<R>
where
    Self: ReprC<CType = CWideBox<R>>,
    <<R as Wide>::Header as WideHeader>::Data:
        ReprC<CType = <<P as Wide>::Header as WideHeader>::Data>,
    K: Into<CBox<<<R as Wide>::Header as WideHeader>::Data>>,
{
    type Error = <U as TryInto<<P as Wide>::Metadata>>::Error;
    unsafe fn pack(data: K, metadata: U) -> Result<Self::CType, Self::Error> {
        let data: CBox<<<R as Wide>::Header as WideHeader>::Data> = data.into();
        Ok(unsafe { CWideBox::from_raw_nullable(data.as_ptr(), metadata.try_into()?) })
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

impl<A, B, Part1: CType, Part2: CType> Unpack2<Part1, Part2> for (A, B)
where
    Self: ReprC<CType = Tuple2<Part1, Part2>>,
{
    type Error = core::convert::Infallible;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        Ok((value.0, value.1))
    }
}

impl<A, B, Part1: CType, Part2: CType> Pack2<Part1, Part2> for (A, B)
where
    Self: ReprC<CType = Tuple2<Part1, Part2>>,
{
    type Error = core::convert::Infallible;

    #[inline(always)]
    unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
        Ok(Tuple2(part1, part2))
    }
}

impl<T: Unpack2<A, B>, A: CType, B: CType> Unpack2<Borrowed<A>, B> for Borrowed<T> {
    type Error = T::Error;

    fn unpack(value: Self::CType) -> Result<(Borrowed<A>, B), Self::Error> {
        T::unpack(Borrowed::into_inner(value))
            .map(|(data, metadata)| (Borrowed::new(data), metadata))
    }
}

impl<T: Pack2<A, B>, A: CType, B: CType> Pack2<Borrowed<A>, B> for Borrowed<T> {
    type Error = T::Error;

    unsafe fn pack(a: Borrowed<A>, b: B) -> Result<Self::CType, Self::Error> {
        unsafe { T::pack(Borrowed::into_inner(a), b) }.map(Borrowed::new)
    }
}
