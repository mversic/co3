//! Conversion between C-compatible representations and ABI arguments.

use core::{
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    mem::ManuallyDrop,
};

use disjoint_impls::disjoint_impls;

use crate::{
    CType, ReprC,
    num::NonZero,
    option::COptionSome,
    result::{CResultErr, CResultOk},
    tuple::CTuple2,
    wide::{MutWideRefCarrier, PointeeData, Wide, WideHeader, WideRefCarrier},
};
#[cfg(feature = "alloc")]
use crate::{boxed::Box, wide::CWideBox};

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
        impl<R: ReprC<CType = P> + ?Sized, P: Wide<Header: WideHeader<Data: CType>, Metadata: TryInto<U>> + CType + ?Sized, U: CType>
            Unpack2<*$pointer <P::Header as WideHeader>::Data, U> for $source
        {
            type Error = <P::Metadata as TryInto<U>>::Error;

            #[inline(always)]
            fn unpack(value: Self::CType) -> Result<(*$pointer <P::Header as WideHeader>::Data, U), Self::Error> {
                $(let value = value.$field;)?
                Ok((P::$accessor(value).cast(), P::metadata(value).try_into()?))
            }
        }
        // TODO: Implement Pack2
    };
}

impl_raw_wide_unpack!(*const R, *const, as_ptr);
impl_raw_wide_unpack!(*mut R, *mut, as_mut_ptr);
impl_raw_wide_unpack!(NonZero<*mut R>, *const, as_ptr, 0);
impl_raw_wide_unpack!(NonZero<*mut R>, *mut, as_mut_ptr, 0);

macro_rules! impl_ref_wide_pack {
    ($reference:ty) => {
        impl<
            R: ReprC<CType = P> + Wide + ?Sized,
            P: Wide<Metadata: TryInto<U>> + ?Sized,
            U: CType,
            Pointer: CType,
        > Unpack2<Pointer, U> for $reference
        where
            Self: ReprC<CType: WideRefCarrier<R, Pointer = Pointer>>,
        {
            type Error = <P::Metadata as TryInto<U>>::Error;
            #[inline(always)]
            fn unpack(value: Self::CType) -> Result<(Pointer, U), Self::Error> {
                Ok((value.as_ptr(), value.metadata().try_into()?))
            }
        }
        impl<
            R: ReprC<CType = P> + Wide + ?Sized,
            P: Wide + ?Sized,
            U: CType + TryInto<P::Metadata>,
            Pointer: CType,
        > Pack2<Pointer, U> for $reference
        where
            Self: ReprC<CType: WideRefCarrier<R, Pointer = Pointer>>,
        {
            type Error = <U as TryInto<P::Metadata>>::Error;
            #[inline(always)]
            unsafe fn pack(data: Pointer, metadata: U) -> Result<Self::CType, Self::Error> {
                Ok(unsafe { Self::CType::from_parts(data, metadata.try_into()?) })
            }
        }
    };
}
impl_ref_wide_pack!(&R);
impl<R, P, U> Unpack2<*mut PointeeData<P>, U> for &mut R
where
    R: ReprC<CType = P> + Wide + ?Sized,
    P: Wide<Header: WideHeader<Data: CType>, Metadata: TryInto<U>> + ?Sized,
    U: CType,
    Self: ReprC<CType: MutWideRefCarrier<R>>,
{
    type Error = <P::Metadata as TryInto<U>>::Error;

    fn unpack(value: Self::CType) -> Result<(*mut PointeeData<P>, U), Self::Error> {
        Ok((value.data(), value.metadata().try_into()?))
    }
}

impl<R, P, U> Pack2<*mut PointeeData<P>, U> for &mut R
where
    R: ReprC<CType = P> + Wide + ?Sized,
    P: Wide<Header: WideHeader<Data: CType>> + ?Sized,
    U: CType + TryInto<P::Metadata>,
    Self: ReprC<CType: MutWideRefCarrier<R>>,
{
    type Error = <U as TryInto<P::Metadata>>::Error;

    unsafe fn pack(data: *mut PointeeData<P>, metadata: U) -> Result<Self::CType, Self::Error> {
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

impl<T: Pack2<Part1, Part2>, Part1: CType, Part2: CType> Pack2<Part1, Part2> for Option<T>
where
    Self: ReprC<CType = COptionSome<T>>,
{
    type Error = T::Error;

    #[inline(always)]
    unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
        unsafe { T::pack(part1, part2) }.map(COptionSome)
    }
}

impl<T: Unpack2<Part1, Part2>, Part1: CType, Part2: CType> Unpack2<Part1, Part2> for Option<T>
where
    Self: ReprC<CType = COptionSome<T>>,
{
    type Error = T::Error;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        T::unpack(value.0)
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

    impl<Ok: Unpack2<A, B> + crate::niche::Niche, Err: ReprC, A: CType, B: CType>
        Unpack2<A, B> for Result<Ok, Err>
    where
        Ok: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        Err: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: ReprC<CType = CResultOk<Ok, Err>>,
    {
        type Error = Ok::Error;
        fn unpack(value: Self::CType) -> Result<(A, B), Self::Error> {
            Ok::unpack(value.0)
        }
    }

    impl<Ok: ReprC, Err: Unpack2<A, B> + crate::niche::Niche, A: CType, B: CType>
        Unpack2<A, B> for Result<Ok, Err>
    where
        Ok: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Err: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        Self: ReprC<CType = CResultErr<Ok, Err>>,
    {
        type Error = Err::Error;
        fn unpack(value: Self::CType) -> Result<(A, B), Self::Error> {
            Err::unpack(value.0)
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

    impl<Ok: Pack2<A, B> + crate::niche::Niche, Err: ReprC, A: CType, B: CType>
        Pack2<A, B> for Result<Ok, Err>
    where
        Ok: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        Err: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Self: ReprC<CType = CResultOk<Ok, Err>>,
    {
        type Error = Ok::Error;
        unsafe fn pack(a: A, b: B) -> Result<Self::CType, Self::Error> {
            unsafe { Ok::pack(a, b) }.map(|value| CResultOk(value, PhantomData))
        }
    }

    impl<Ok: ReprC, Err: Pack2<A, B> + crate::niche::Niche, A: CType, B: CType>
        Pack2<A, B> for Result<Ok, Err>
    where
        Ok: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Zero>>,
        Err: rust_spec::RustSpec<Size = rust_spec::size::Sized<rust_spec::Gt<rust_spec::Zero>>>,
        Self: ReprC<CType = CResultErr<Ok, Err>>,
    {
        type Error = Err::Error;
        unsafe fn pack(a: A, b: B) -> Result<Self::CType, Self::Error> {
            unsafe { Err::pack(a, b) }.map(|value| CResultErr(value, PhantomData))
        }
    }
}

#[cfg(feature = "alloc")]
impl<
    R: ReprC<CType = P> + Wide + ?Sized,
    P: Wide<Header: WideHeader<Data: CType + Sized>, Metadata: TryInto<U>> + ?Sized,
    U: CType,
> Unpack2<*mut <P::Header as WideHeader>::Data, U> for Box<R>
where
    Self: ReprC<CType = CWideBox<R>>,
    <<R as Wide>::Header as WideHeader>::Data:
        ReprC<CType = <<P as Wide>::Header as WideHeader>::Data>,
{
    type Error = <<P as Wide>::Metadata as TryInto<U>>::Error;
    fn unpack(
        value: Self::CType,
    ) -> Result<(*mut <P::Header as WideHeader>::Data, U), Self::Error> {
        Ok((value.as_ptr(), value.metadata().try_into()?))
    }
}

#[cfg(feature = "alloc")]
impl<
    R: ReprC<CType = P> + Wide + ?Sized,
    P: Wide<Header: WideHeader<Data: CType + Sized>> + ?Sized,
    U: CType + TryInto<<P as Wide>::Metadata>,
> Pack2<*mut <P::Header as WideHeader>::Data, U> for Box<R>
where
    Self: ReprC<CType = CWideBox<R>>,
    <<R as Wide>::Header as WideHeader>::Data:
        ReprC<CType = <<P as Wide>::Header as WideHeader>::Data>,
{
    type Error = <U as TryInto<<P as Wide>::Metadata>>::Error;
    unsafe fn pack(
        data: *mut <P::Header as WideHeader>::Data,
        metadata: U,
    ) -> Result<Self::CType, Self::Error> {
        Ok(unsafe { CWideBox::from_raw_nullable(data, metadata.try_into()?) })
    }
}

impl<A: ReprC<CType = Part1>, B: ReprC<CType = Part2>, Part1: CType, Part2: CType>
    Unpack2<Part1, Part2> for (A, B)
where
    Self: ReprC<CType = CTuple2<A, B>>,
{
    type Error = core::convert::Infallible;

    #[inline(always)]
    fn unpack(value: Self::CType) -> Result<(Part1, Part2), Self::Error> {
        Ok((value.0, value.1))
    }
}

impl<A: ReprC<CType = Part1>, B: ReprC<CType = Part2>, Part1: CType, Part2: CType>
    Pack2<Part1, Part2> for (A, B)
where
    Self: ReprC<CType = CTuple2<A, B>>,
{
    type Error = core::convert::Infallible;

    #[inline(always)]
    unsafe fn pack(part1: Part1, part2: Part2) -> Result<Self::CType, Self::Error> {
        Ok(CTuple2(part1, part2))
    }
}

// Borrowed ownership changes the marker, not the unpacked ABI parts.
impl<T: Unpack2<A, B>, A: CType, B: CType> Unpack2<A, B> for crate::borrow::CBorrowed<T> {
    type Error = T::Error;
    fn unpack(value: Self::CType) -> Result<(A, B), Self::Error> {
        T::unpack(value.0)
    }
}
impl<T: Pack2<A, B>, A: CType, B: CType> Pack2<A, B> for crate::borrow::CBorrowed<T> {
    type Error = T::Error;
    unsafe fn pack(a: A, b: B) -> Result<Self::CType, Self::Error> {
        unsafe { T::pack(a, b) }.map(crate::borrow::CBorrowed)
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
        assert!(carrier.0.as_ptr().is_null());
        let (pointer, unpacked) =
            <Option<Box<Pointee>> as Unpack2<*mut u8, usize>>::unpack(carrier).unwrap();
        assert!(pointer.is_null());
        assert_eq!(unpacked, metadata);
    }
}
