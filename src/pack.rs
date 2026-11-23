//! Conversion between C-compatible representations and ABI arguments.

use core::{
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    mem::ManuallyDrop,
    ptr::NonNull,
};

use disjoint_impls::disjoint_impls;

use crate::{
    CType, ReprC,
    marker::{Native, NonZero},
    option::abi::OptionSome as AbiOptionSome,
    result::abi::{ResultErr as AbiResultErr, ResultOk as AbiResultOk},
    tuple::abi::Tuple2 as AbiTuple2,
    wide::{
        PointeeData, Wide, WideHeader,
        abi::{WideRefCarrier, WideRefMutCarrier},
    },
};
#[cfg(feature = "alloc")]
use crate::{boxed::Box, wide::abi::WideBox as AbiWideBox};

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
    ($source:ty, *$pointer:ident, $accessor:ident $(, $field:tt)?) => {
        impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>, Metadata: TryInto<U>>> + ?Sized, U: CType>
            Unpack2<*$pointer <<R::CType as Wide>::Header as WideHeader>::Data, U> for $source
        {
            type Error = <<R::CType as Wide>::Metadata as TryInto<U>>::Error;

            #[inline(always)]
            fn unpack(value: Self::CType) -> Result<(*$pointer <<R::CType as Wide>::Header as WideHeader>::Data, U), Self::Error> {
                $(let value = value.$field;)?
                Ok((<R::CType>::$accessor(value).cast(), R::CType::metadata(value).try_into()?))
            }
        }
        // TODO: Implement Pack2
    };
}

impl_raw_wide_unpack!(*const R, *const, as_ptr);
impl_raw_wide_unpack!(*mut R, *mut, as_mut_ptr);
impl_raw_wide_unpack!(NonNull<R>, *const, as_ptr, 0);
impl_raw_wide_unpack!(NonNull<R>, *mut, as_mut_ptr, 0);
impl_raw_wide_unpack!(NonZero<*mut R>, *const, as_ptr, 0);
impl_raw_wide_unpack!(NonZero<*mut R>, *mut, as_mut_ptr, 0);

impl<R, U> Unpack2<*const PointeeData<R::CType>, U> for &R
where
    R: ReprC<CType: Wide<Header: WideHeader<Data: CType>, Metadata: TryInto<U>>> + Wide + ?Sized,
    U: CType,
    Self: ReprC<CType: WideRefCarrier<Pointee = <R as ReprC>::CType>>,
{
    type Error = <<R::CType as Wide>::Metadata as TryInto<U>>::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(*const PointeeData<R::CType>, U), Self::Error> {
        Ok((value.data(), value.metadata().try_into()?))
    }
}

impl<R, U> Pack2<*const PointeeData<R::CType>, U> for &R
where
    R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + Wide + ?Sized,
    U: CType + TryInto<<R::CType as Wide>::Metadata>,
    Self: ReprC<CType: WideRefCarrier<Pointee = <R as ReprC>::CType>>,
{
    type Error = <U as TryInto<<R::CType as Wide>::Metadata>>::Error;

    #[inline(always)]
    unsafe fn pack(
        data: *const PointeeData<R::CType>,
        metadata: U,
    ) -> Result<Self::CType, Self::Error> {
        Ok(unsafe { Self::CType::from_raw_parts(data, metadata.try_into()?) })
    }
}

impl<R, U> Unpack2<*mut PointeeData<R::CType>, U> for &mut R
where
    R: ReprC<CType: Wide<Header: WideHeader<Data: CType>, Metadata: TryInto<U>>> + Wide + ?Sized,
    U: CType,
    Self: ReprC<CType: WideRefMutCarrier<Pointee = <R as ReprC>::CType>>,
{
    type Error = <<R::CType as Wide>::Metadata as TryInto<U>>::Error;

    fn unpack(value: Self::CType) -> Result<(*mut PointeeData<R::CType>, U), Self::Error> {
        Ok((value.data(), value.metadata().try_into()?))
    }
}

impl<R, U> Pack2<*mut PointeeData<R::CType>, U> for &mut R
where
    R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + Wide + ?Sized,
    U: CType + TryInto<<R::CType as Wide>::Metadata>,
    Self: ReprC<CType: WideRefMutCarrier<Pointee = <R as ReprC>::CType>>,
{
    type Error = <U as TryInto<<R::CType as Wide>::Metadata>>::Error;

    unsafe fn pack(
        data: *mut PointeeData<R::CType>,
        metadata: U,
    ) -> Result<Self::CType, Self::Error> {
        Ok(unsafe { Self::CType::from_raw_parts_mut(data, metadata.try_into()?) })
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

impl<T: Pack2<Part1, Part2> + crate::niche::Niche, Part1: CType, Part2: CType> Pack2<Part1, Part2>
    for Option<T>
where
    Self: ReprC<CType = Native<crate::option::abi::OptionSome<<T as ReprC>::CType>>>,
{
    type Error = T::Error;

    #[inline(always)]
    unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
        unsafe { T::pack(part1, part2) }.map(|value| Native(AbiOptionSome(value)))
    }
}

impl<T: Unpack2<Part1, Part2> + crate::niche::Niche, Part1: CType, Part2: CType>
    Unpack2<Part1, Part2> for Option<T>
where
    Self: ReprC<CType = Native<crate::option::abi::OptionSome<<T as ReprC>::CType>>>,
{
    type Error = T::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        T::unpack(value.0.0)
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

    impl<Ok: Unpack2<A, B> + crate::niche::Niche, Err: ReprC, A: CType, B: CType> Unpack2<A, B>
        for Result<Ok, Err>
    where
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<Ok as ReprC>::CType, <Err as ReprC>::CType>>,
        >,
        Ok: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        Err: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        <Err as ReprC>::CType: Sized,
    {
        type Error = Ok::Error;
        fn unpack(value: Self::CType) -> Result<(A, B), Self::Error> {
            Ok::unpack(value.0.0)
        }
    }

    impl<Ok: ReprC, Err: Unpack2<A, B> + crate::niche::Niche, A: CType, B: CType> Unpack2<A, B>
        for Result<Ok, Err>
    where
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<Ok as ReprC>::CType, <Err as ReprC>::CType>>,
        >,
        Ok: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Err: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        <Ok as ReprC>::CType: Sized,
    {
        type Error = Err::Error;
        fn unpack(value: Self::CType) -> Result<(A, B), Self::Error> {
            Err::unpack(value.0.0)
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

    impl<Ok: Pack2<A, B> + crate::niche::Niche, Err: ReprC, A: CType, B: CType> Pack2<A, B>
        for Result<Ok, Err>
    where
        Self: ReprC<
            CType = Native<crate::result::abi::ResultOk<<Ok as ReprC>::CType, <Err as ReprC>::CType>>,
        >,
        Ok: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        Err: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        <Err as ReprC>::CType: Sized,
    {
        type Error = Ok::Error;
        unsafe fn pack(a: A, b: B) -> Result<Self::CType, Self::Error> {
            unsafe { Ok::pack(a, b) }.map(|value| Native(AbiResultOk(value, PhantomData)))
        }
    }

    impl<Ok: ReprC, Err: Pack2<A, B> + crate::niche::Niche, A: CType, B: CType> Pack2<A, B>
        for Result<Ok, Err>
    where
        Self: ReprC<
            CType = Native<crate::result::abi::ResultErr<<Ok as ReprC>::CType, <Err as ReprC>::CType>>,
        >,
        Ok: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Err: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        <Ok as ReprC>::CType: Sized,
    {
        type Error = Err::Error;
        unsafe fn pack(a: A, b: B) -> Result<Self::CType, Self::Error> {
            unsafe { Err::pack(a, b) }.map(|value| Native(AbiResultErr(value, PhantomData)))
        }
    }
}

#[cfg(feature = "alloc")]
impl<
    R: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>, Metadata: TryInto<U>>>
        + Wide
        + ?Sized,
    U: CType,
> Unpack2<*mut <<R::CType as Wide>::Header as WideHeader>::Data, U> for Box<R>
where
    Self: ReprC<CType = AbiWideBox<<R as ReprC>::CType>>,
    <<R as Wide>::Header as WideHeader>::Data:
        ReprC<CType = <<<R as ReprC>::CType as Wide>::Header as WideHeader>::Data>,
{
    type Error = <<R::CType as Wide>::Metadata as TryInto<U>>::Error;
    fn unpack(
        value: Self::CType,
    ) -> Result<(*mut <<R::CType as Wide>::Header as WideHeader>::Data, U), Self::Error> {
        Ok((value.as_ptr(), value.metadata().try_into()?))
    }
}

#[cfg(feature = "alloc")]
impl<
    R: ReprC<CType: Wide<Header: WideHeader<Data: CType + Sized>>> + Wide + ?Sized,
    U: CType + TryInto<<R::CType as Wide>::Metadata>,
> Pack2<*mut <<R::CType as Wide>::Header as WideHeader>::Data, U> for Box<R>
where
    Self: ReprC<CType = AbiWideBox<<R as ReprC>::CType>>,
    <<R as Wide>::Header as WideHeader>::Data:
        ReprC<CType = <<<R as ReprC>::CType as Wide>::Header as WideHeader>::Data>,
{
    type Error = <U as TryInto<<R::CType as Wide>::Metadata>>::Error;
    unsafe fn pack(
        data: *mut <<R::CType as Wide>::Header as WideHeader>::Data,
        metadata: U,
    ) -> Result<Self::CType, Self::Error> {
        Ok(unsafe { AbiWideBox::from_raw_nullable(data, metadata.try_into()?) })
    }
}

impl<A: ReprC<CType = Part1>, B: ReprC<CType = Part2>, Part1: CType, Part2: CType>
    Unpack2<Part1, Part2> for (A, B)
where
    Self: ReprC<CType = Native<AbiTuple2<<A as ReprC>::CType, <B as ReprC>::CType>>>,
{
    type Error = core::convert::Infallible;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        Ok((value.0.0, value.0.1))
    }
}

impl<A: ReprC<CType = Part1>, B: ReprC<CType = Part2>, Part1: CType, Part2: CType>
    Pack2<Part1, Part2> for (A, B)
where
    Self: ReprC<CType = Native<AbiTuple2<<A as ReprC>::CType, <B as ReprC>::CType>>>,
{
    type Error = core::convert::Infallible;

    #[inline(always)]
    unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
        Ok(Native(AbiTuple2(part1, part2)))
    }
}

macro_rules! impl_transparent_pack2 {
    ($($wrapper:ident),+ $(,)?) => {$ (
        impl<T: Unpack2<A, B>, A: CType, B: CType> Unpack2<A, B> for $wrapper<T> {
            type Error = T::Error;

            #[inline(always)]
            fn unpack(value: Self::CType) -> Result<(A, B), Self::Error> {
                T::unpack($wrapper::into_inner(value))
            }
        }

        impl<T: Pack2<A, B>, A: CType, B: CType> Pack2<A, B> for $wrapper<T> {
            type Error = T::Error;

            #[inline(always)]
            unsafe fn pack(a: A, b: B) -> Result<Self::CType, Self::Error> {
                unsafe { T::pack(a, b) }.map($wrapper::new)
            }
        }
    )*};
}

impl_transparent_pack2!(ManuallyDrop, Cell, UnsafeCell);

#[cfg(test)]
mod tests {
    use core::ptr::NonNull;

    #[cfg(feature = "alloc")]
    use super::{Box, Pack2};
    use super::{NonZero, Unpack2};
    use crate::wide::tests::{Custom, Metadata};

    #[test]
    fn raw_wide_unpack_preserves_custom_metadata() {
        type Pointee = Custom;
        let mut bytes = [1_u8, 2, 3];
        let ptr = bytes.as_mut_slice() as *mut [u8] as *mut Pointee;
        let expected = Metadata {
            length: 3,
            marker: 7,
        };
        let nonnull = NonNull::new(ptr).unwrap();

        let (data, metadata) =
            <*const Pointee as Unpack2<*const u8, Metadata>>::unpack(ptr).unwrap();
        assert_eq!((data, metadata), (bytes.as_ptr(), expected));
        let (data, metadata) = <*mut Pointee as Unpack2<*mut u8, Metadata>>::unpack(ptr).unwrap();
        assert_eq!((data, metadata), (bytes.as_mut_ptr(), expected));
        let (data, metadata) =
            <NonZero<*mut Pointee> as Unpack2<*const u8, Metadata>>::unpack(crate::encode(nonnull))
                .unwrap();
        assert_eq!((data, metadata), (bytes.as_ptr(), expected));
        let (data, metadata) =
            <NonZero<*mut Pointee> as Unpack2<*mut u8, Metadata>>::unpack(crate::encode(nonnull))
                .unwrap();
        assert_eq!((data, metadata), (bytes.as_mut_ptr(), expected));
    }

    #[cfg(feature = "alloc")]
    #[test]
    fn owned_pack_preserves_null_pointer_and_metadata() {
        type Pointee = [u8];
        let metadata = 3_usize;
        let carrier = unsafe {
            <Option<Box<Pointee>> as Pack2<*mut u8, usize>>::pack(core::ptr::null_mut(), metadata)
        }
        .unwrap();
        assert!(carrier.0.0.as_ptr().is_null());
        let (pointer, unpacked) =
            <Option<Box<Pointee>> as Unpack2<*mut u8, usize>>::unpack(carrier).unwrap();
        assert!(pointer.is_null());
        assert_eq!(unpacked, metadata);
    }

    #[test]
    fn exclusive_unsafe_cell_wide_uses_mutable_carrier() {
        use core::cell::UnsafeCell;

        let mut cell = UnsafeCell::new([1u8, 2, 3]);
        let original: &mut UnsafeCell<[u8]> = &mut cell;

        let carrier: crate::wide::abi::WideRefMut<'_, <UnsafeCell<[u8]> as crate::ReprC>::CType> =
            crate::encode(original);
        let (data, len) =
            <&mut UnsafeCell<[u8]> as Unpack2<*mut u8, usize>>::unpack(carrier).unwrap();
        assert_eq!(data, cell.get().cast());
        assert_eq!(len, 3);
        unsafe { data.add(1).write(9) };

        assert_eq!(unsafe { &*cell.get() }, &[1, 9, 3]);
    }
}
