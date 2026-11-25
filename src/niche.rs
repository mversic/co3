//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc::boxed::Box;

use disjoint_impls::disjoint_impls;
#[cfg(feature = "repr-rust")]
use rust_spec::{
    RustSpec,
    layout::LayoutKind,
    niche::{WithNiche, WithoutNiche},
};

use crate::{
    CType, ReprC,
    reference::abi::{Ref, RefMut, SoftRef, SoftRefMut},
    wide::{
        Wide, WideHeader, WideRef, WideRefMut,
        abi::{
            SoftWideRef as AbiSoftWideRef, SoftWideRefMut as AbiSoftWideRefMut,
            WideRef as AbiWideRef, WideRefMut as AbiWideRefMut,
        },
    },
};
#[cfg(feature = "alloc")]
use crate::{boxed::abi::Box as AbiBox, wide::abi::WideBox as AbiWideBox};
#[cfg(feature = "repr-rust")]
use crate::{marker::Native, option::abi::Option as AbiOption, tuple::abi::Tuple2 as AbiTuple2};

pub(crate) fn checked_option_payload<T: Niche>(encoded: Option<T::CType>) -> T::CType {
    match encoded {
        Some(value) => {
            debug_assert!(!T::is_niche(&value), "encoding produced NICHE value");
            value
        }
        None => T::NICHE,
    }
}

disjoint_impls! {
    /// Type that has a trap representation that can be used as a niche value.
    ///
    /// # Example
    ///
    /// [`Option<bool>`]     - will be serilized into one byte
    /// [`Option<*const T>`] - will take the size of the pointer
    pub trait Niche: ReprC<CType: Copy> {
        const NICHE: Self::CType;

        /// Returns whether a carrier represents this type's reserved niche.
        ///
        /// This must recognize `NICHE` and must not match any carrier produced
        /// by encoding a valid value of `Self`.
        fn is_niche(value: &Self::CType) -> bool;
    }

    impl<'a, R: ReprC + ?Sized> Niche for &'a R
    where
        Self: ReprC<CType = Ref<'a, <R as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        const NICHE: Self::CType = Ref::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<R: ReprC + ?Sized> Niche for &R
    where
        Self: ReprC<CType = SoftRef<<R as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        const NICHE: Self::CType = SoftRef::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<'a, R: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType>>> + ?Sized> Niche
        for &'a R
    where
        Self: ReprC<CType = AbiWideRef<'a, <R as ReprC>::CType>>,
    {
        const NICHE: Self::CType = AbiWideRef::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<R: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType>>> + ?Sized> Niche for &R
    where
        Self: ReprC<CType = AbiSoftWideRef<<R as ReprC>::CType>>,
    {
        const NICHE: Self::CType = AbiSoftWideRef::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<'a, R: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType>>> + ?Sized> Niche
        for &'a R
    where
        Self: ReprC<CType = AbiWideRefMut<'a, <R as ReprC>::CType>>,
    {
        const NICHE: Self::CType = AbiWideRefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }

    impl<'a, R: ?Sized> Niche for WideRef<'a, R>
    where
        R: ReprC<CType: crate::CType + Wide<Header: CType + WideHeader<Data: CType>>> + Wide,
        Self: ReprC<CType = AbiWideRef<'a, <R as ReprC>::CType>>,
    {
        const NICHE: Self::CType = AbiWideRef::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }

    impl<'a, R: ?Sized> Niche for WideRef<'a, R>
    where
        R: ReprC<CType: crate::CType + Wide<Header: CType + WideHeader<Data: CType>>> + Wide,
        Self: ReprC<CType = AbiSoftWideRef<<R as ReprC>::CType>>,
    {
        const NICHE: Self::CType = AbiSoftWideRef::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }

    impl<'a, R: ?Sized> Niche for WideRefMut<'a, R>
    where
        R: ReprC<CType: crate::CType + Wide<Header: CType + WideHeader<Data: CType>>> + Wide,
        Self: ReprC<CType = AbiWideRefMut<'a, <R as ReprC>::CType>>,
    {
        const NICHE: Self::CType = AbiWideRefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }

    impl<'a, R: ?Sized> Niche for WideRefMut<'a, R>
    where
        R: ReprC<CType: crate::CType + Wide<Header: CType + WideHeader<Data: CType>>> + Wide,
        Self: ReprC<CType = AbiSoftWideRefMut<<R as ReprC>::CType>>,
    {
        const NICHE: Self::CType = AbiSoftWideRefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }

    impl<'a, R: ReprC + ?Sized> Niche for &'a mut R
    where
        Self: ReprC<CType = RefMut<'a, <R as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        const NICHE: Self::CType = RefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<R: ReprC + ?Sized> Niche for &mut R
    where
        Self: ReprC<CType = SoftRefMut<<R as ReprC>::CType>>,
        <R as ReprC>::CType: Sized,
    {
        const NICHE: Self::CType = SoftRefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<'a, R: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType>>> + ?Sized> Niche
        for &'a mut R
    where
        Self: ReprC<CType = AbiWideRefMut<'a, <R as ReprC>::CType>>,
    {
        const NICHE: Self::CType = AbiWideRefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<R: ReprC<CType: crate::CType + Wide<Header: WideHeader<Data: CType>>> + ?Sized> Niche
        for &mut R
    where
        Self: ReprC<CType = AbiSoftWideRefMut<<R as ReprC>::CType>>,
    {
        const NICHE: Self::CType = AbiSoftWideRefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C: CType> Niche for Box<R>
    where
        Self: ReprC<CType = AbiBox<C>>,
    {
        const NICHE: Self::CType = AbiBox::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C: CType + Wide<Header: WideHeader<Data: CType>> + ?Sized> Niche for Box<R>
    where
        Self: ReprC<CType = AbiWideBox<C>>,
    {
        const NICHE: Self::CType = AbiWideBox::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }

    // TODO: Support ?Sized
    //impl<R: ?C> Niche for NonNull<R>
    //where
    //    Self: ReprC<CType = AbiWideBox<C>>,
    //{
    //    const NICHE: <Self as ReprC>::CType = AbiWideBox::none();

    //}

    #[cfg(feature = "repr-rust")]
    impl<R: ReprC> Niche for Option<R>
    where
        Self: ReprC<CType = Native<crate::option::abi::Option<<R as ReprC>::CType>>>,
        <R as ReprC>::CType: Copy,
    {
        const NICHE: Self::CType = Native(AbiOption::niche());

        fn is_niche(value: &Self::CType) -> bool {
            value.is_niche()
        }
    }
    #[cfg(feature = "repr-rust")]
    impl Niche for Option<bool>
    where
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<bool as ReprC>::CType>>>,
    {
        const NICHE: Self::CType = Native(crate::option::abi::OptionSome(crate::primitive::abi::bool(
            3,
        )));

        fn is_niche(value: &Self::CType) -> bool {
            value.0.0.0 == 3
        }
    }

    #[cfg(feature = "repr-rust")]
    impl Niche for Option<Option<bool>>
    where
        Self: ReprC<CType = Native<crate::option::abi::OptionSome<<Option<bool> as ReprC>::CType>>>,
    {
        const NICHE: Self::CType = Native(crate::option::abi::OptionSome(Native(
            crate::option::abi::OptionSome(crate::primitive::abi::bool(4)),
        )));

        fn is_niche(value: &Self::CType) -> bool {
            value.0.0.0.0.0 == 4
        }
    }

    #[cfg(feature = "repr-rust")]
    impl<A: Niche, B: ReprC, N: LayoutKind> Niche for (A, B)
    where
        A: RustSpec<Niche = WithNiche<N>>,
        <B as ReprC>::CType: Copy,
    {
        const NICHE: Self::CType = Native(AbiTuple2(A::NICHE, unsafe { core::mem::zeroed() }));

        fn is_niche(value: &Self::CType) -> bool {
            A::is_niche(&value.0.0)
        }
    }

    #[cfg(feature = "repr-rust")]
    impl<A: ReprC, B: Niche, N: LayoutKind> Niche for (A, B)
    where
        A: RustSpec<Niche = WithoutNiche>,
        B: RustSpec<Niche = WithNiche<N>>,
        <A as ReprC>::CType: Copy,
    {
        const NICHE: Self::CType = Native(AbiTuple2(unsafe { core::mem::zeroed() }, B::NICHE));

        fn is_niche(value: &Self::CType) -> bool {
            B::is_niche(&value.0.1)
        }
    }
}

impl<R: Niche, const N: usize> Niche for [R; N] {
    const NICHE: Self::CType = {
        assert!(N != 0, "an empty array has no niche");
        [R::NICHE; N]
    };

    fn is_niche(value: &Self::CType) -> bool {
        N != 0 && value.iter().all(R::is_niche)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::string::String;
    use core::{mem::ManuallyDrop, num::NonZero, ptr::NonNull};

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::{
        CType,
        convert::{Decode, Encode},
        lang,
    };

    #[test]
    fn nested_option_niche_family() {
        assert_impl_all!(Option<bool>:
            Niche<CType = Native<crate::option::abi::OptionSome<<bool as crate::ReprC>::CType>>>,
            Decode<'static>,
            Encode,

        );
        assert_impl_all!(Option<Option<bool>>:
            Niche<CType = Native<crate::option::abi::OptionSome<<Option<bool> as crate::ReprC>::CType>>>,
            Decode<'static>,
            Encode,

        );
        assert_impl_all!(Option<(u8, NonZero<u8>)>:
            ReprC<CType = Native<crate::option::abi::OptionSome<<(u8, NonZero<u8>) as crate::ReprC>::CType>>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(Option<bool>: CType);
        assert_not_impl_any!(Option<Option<bool>>: CType);
    }

    #[test]
    fn niche_predicates_do_not_depend_on_carrier_equality() {
        use core::cmp::{Ordering, Reverse};

        assert!(bool::is_niche(&bool::NICHE));
        assert!(char::is_niche(&char::NICHE));
        assert!(Ordering::is_niche(&Ordering::NICHE));
        assert!(<Reverse<bool>>::is_niche(&<Reverse<bool>>::NICHE));
        assert!(<(u8, bool)>::is_niche(&<(u8, bool)>::NICHE));
        assert!(<Option<bool>>::is_niche(&<Option<bool>>::NICHE));
        assert!(<Option<Option<bool>>>::is_niche(
            &<Option<Option<bool>>>::NICHE
        ));
        assert!(!bool::is_niche(&crate::primitive::abi::bool(0)));
        assert_eq!(
            unsafe {
                crate::decode::<Option<bool>, lang::C>(crate::encode::<_, lang::C>(None::<bool>))
            },
            Some(None)
        );
        assert_eq!(
            unsafe {
                crate::decode::<Option<Ordering>, lang::C>(crate::encode::<_, lang::C>(
                    None::<Ordering>,
                ))
            },
            Some(None)
        );
        assert_eq!(
            unsafe {
                crate::decode::<Option<char>, lang::C>(crate::encode::<_, lang::C>(None::<char>))
            },
            Some(None)
        );
        assert_eq!(
            unsafe {
                crate::decode::<Option<crate::option::Option<u32>>, lang::C>(crate::encode::<
                    _,
                    lang::C,
                >(
                    None::<crate::option::Option<u32>>,
                ))
            },
            Some(None)
        );
        assert_eq!(
            unsafe {
                crate::decode::<Option<Option<bool>>, lang::C>(crate::encode::<_, lang::C>(
                    None::<Option<bool>>,
                ))
            },
            Some(None)
        );
        assert_eq!(
            unsafe {
                crate::decode::<Option<(u8, bool)>, lang::C>(crate::encode::<_, lang::C>(
                    None::<(u8, bool)>,
                ))
            },
            Some(None)
        );
        assert_eq!(
            unsafe {
                crate::decode::<Result<bool, ()>, lang::C>(crate::encode::<_, lang::C>(Err::<
                    bool,
                    (),
                >(
                    ()
                )))
            },
            Some(Err(()))
        );
    }

    #[test]
    fn reserved_pointer_and_nonzero_carriers_are_not_reflexive() {
        fn check<T: PartialEq + PartialOrd>(value: T) {
            assert!(!(value == value));
            assert!(value.partial_cmp(&value).is_none());
        }

        check(crate::marker::NonZero(0u32));
        check(crate::marker::NonZero(core::ptr::null_mut::<u32>()));
        check(SoftRef::<u8>::null());
        check(SoftRefMut::<u8>::null());
        check(AbiSoftWideRef::<[u8]>::null());
        check(AbiSoftWideRefMut::<[u8]>::null());
        #[cfg(feature = "alloc")]
        {
            check(AbiBox::<u8>::null());
            check(AbiWideBox::<[u8]>::null());
            check(crate::vec::abi::Vec::<u8>::null());
            check(crate::string::abi::String::null());
        }

        assert!(crate::marker::NonZero(1u32) == crate::marker::NonZero(1u32));
        assert!(crate::marker::NonZero(1u32) < crate::marker::NonZero(2u32));
        assert_eq!(
            unsafe {
                crate::decode::<Option<core::num::NonZeroU32>, lang::C>(
                    crate::encode::<_, lang::C>(None::<core::num::NonZeroU32>),
                )
            },
            Some(None)
        );
        #[cfg(feature = "alloc")]
        assert_eq!(
            unsafe {
                crate::decode::<Option<Box<u8>>, lang::C>(crate::encode::<_, lang::C>(
                    None::<Box<u8>>,
                ))
            },
            Some(None)
        );
        let value = 7u8;
        let reference = SoftRef::<u8>::from_ref(&value);
        assert!(reference == reference);
        assert!(crate::marker::NonZero::new(reference).is_none());
        check(crate::marker::NonZero(reference));
    }

    #[test]
    fn niche_values() {
        assert!(<&bool>::is_niche(
            &unsafe { crate::encode::<_, lang::C>(None::<&bool>) }.0.0
        ));
        assert!(<&mut bool>::is_niche(
            &unsafe {
                crate::soft_encode::<_, lang::C>(None::<&mut bool>, &mut Default::default())
            }
            .0
            .0
        ));

        #[cfg(feature = "alloc")]
        assert!(String::is_niche(
            &unsafe { crate::encode::<_, lang::C>(None::<String>) }.0.0
        ));
        #[cfg(feature = "alloc")]
        assert!(<Box<str>>::is_niche(
            &unsafe { crate::encode::<_, lang::C>(None::<Box<str>>) }.0.0
        ));

        assert!(<&str>::is_niche(
            &unsafe { crate::encode::<_, lang::C>(None::<&str>) }.0.0
        ));

        #[cfg(feature = "alloc")]
        assert!(<&mut str>::is_niche(
            &unsafe { crate::soft_encode::<_, lang::C>(None::<&mut str>, &mut Default::default()) }
                .0
                .0
        ));

        assert_eq!(
            core::ptr::null_mut(),
            unsafe { crate::encode::<_, lang::C>(None::<NonNull<u32>>) }
                .0
                .0
                .0
        );

        #[cfg(feature = "alloc")]
        assert!(<ManuallyDrop<String>>::is_niche(
            &core::mem::ManuallyDrop::new(<String as Niche>::NICHE)
        ));

        assert!(<ManuallyDrop<bool> as Niche>::is_niche(
            &unsafe { crate::encode::<_, lang::C>(None::<ManuallyDrop<bool>>) }
                .0
                .0
        ));
    }
}
