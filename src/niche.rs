//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::{mem::ManuallyDrop, ptr::NonNull};

use disjoint_impls::disjoint_impls;
use rust_spec::{
    RustSpec,
    drop::NoDrop,
    layout::LayoutKind,
    niche::{WithNiche, WithoutNiche},
};

use crate::{
    CType, NeedsDrop, ReprC,
    option::{COption, COptionSome},
    primitives::CBool,
    reference::{CRef, CSoftRefMut, CSoftRef},
    result::CResult,
    tuple::Tuple2,
    wide::{CSoftWideRefMut, CSoftWideRef},
    wide::{Wide, WideHeader},
};
#[cfg(feature = "alloc")]
use crate::{boxed::CBox, wide::CWideBox};

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

    impl<'a, R: ReprC<CType: CType + Sized> + ?Sized> Niche for &'a R
    where
        Self: ReprC<CType = CRef<'a, R>>,
    {
        const NICHE: Self::CType = CRef::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<R: ReprC<CType: CType + Sized> + ?Sized> Niche for &R
    where
        Self: ReprC<CType = CSoftRef<R>>,
    {
        const NICHE: Self::CType = CSoftRef::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<R: ReprC<CType: CType + Sized> + ?Sized> Niche for &R
    where
        Self: ReprC<CType = CSoftRefMut<R, false>>,
    {
        const NICHE: Self::CType = CSoftRefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> Niche for &R
    where
        Self: ReprC<CType = CSoftWideRef<R>>,
    {
        const NICHE: Self::CType = CSoftWideRef::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> Niche for &R
    where
        Self: ReprC<CType = CSoftWideRefMut<R, false>>,
    {
        const NICHE: Self::CType = CSoftWideRefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }

    impl<R: ReprC<CType: CType + Sized> + ?Sized> Niche for &mut R
    where
        Self: ReprC<CType = CSoftRefMut<R, true>>,
    {
        const NICHE: Self::CType = CSoftRefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    impl<R: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> Niche for &mut R
    where
        Self: ReprC<CType = CSoftWideRefMut<R, true>>,
    {
        const NICHE: Self::CType = CSoftWideRefMut::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C: ReprC<CType: CType + Sized>> Niche for Box<R>
    where
        Self: ReprC<CType = CBox<C>>,
    {
        const NICHE: Self::CType = CBox::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C: ReprC<CType: Wide<Header: WideHeader<Data: CType>>> + ?Sized> Niche for Box<R>
    where
        Self: ReprC<CType = CWideBox<C>>,
    {
        const NICHE: Self::CType = CWideBox::null();

        fn is_niche(value: &Self::CType) -> bool {
            value.as_ptr().is_null()
        }
    }

    impl<T: ?Sized, C: CType> Niche for NonNull<T>
    where
        Self: ReprC<CType = crate::NonZero<*mut C>>,
    {
        const NICHE: Self::CType = crate::NonZero(core::ptr::null_mut());

        fn is_niche(value: &Self::CType) -> bool {
            value.0.is_null()
        }
    }
    // TODO: Support ?Sized
    //impl<R: ?C> Niche for NonNull<R>
    //where
    //    Self: ReprC<CType = CWideBox<C>>,
    //{
    //    const NICHE: Self::CType = CWideBox::none();

    //}

    impl<R: ReprC<CType: CType + Copy>> Niche for Option<R>
    where
        Self: ReprC<CType = COption<R>>,
    {
        const NICHE: Self::CType = COption::niche();

        fn is_niche(value: &Self::CType) -> bool {
            value.is_niche()
        }
    }
    // TODO: Depends on: https://github.com/mversic/co3/issues/33
    impl Niche for Option<bool>
    where
        Self: ReprC<CType = COptionSome<bool>>,
    {
        const NICHE: Self::CType = COptionSome::from_carrier(CBool(3));

        fn is_niche(value: &Self::CType) -> bool {
            value.carrier().0 == 3
        }
    }
    impl Niche for Option<Option<bool>>
    where
        Self: ReprC<CType = COptionSome<Option<bool>>>,
    {
        const NICHE: Self::CType = COptionSome::from_carrier(COptionSome::from_carrier(CBool(4)));

        fn is_niche(value: &Self::CType) -> bool {
            value.carrier().carrier().0 == 4
        }
    }

    impl<R: Niche> Niche for ManuallyDrop<R>
    where
        R: RustSpec<Drop = NoDrop>,
    {
        const NICHE: Self::CType = R::NICHE;

        fn is_niche(value: &Self::CType) -> bool {
            R::is_niche(value)
        }
    }
    impl<R: Niche> Niche for ManuallyDrop<R>
    where
        R: RustSpec<Drop: NeedsDrop>,
    {
        const NICHE: Self::CType = ManuallyDrop::new(R::NICHE);

        fn is_niche(value: &Self::CType) -> bool {
            R::is_niche(value)
        }
    }

    impl<A: Niche<CType: Copy>, B: ReprC<CType: Copy>, N: LayoutKind> Niche for (A, B)
    where
        A: RustSpec<Niche = WithNiche<N>>,
    {
        const NICHE: Self::CType = Tuple2(A::NICHE, unsafe { core::mem::zeroed() });

        fn is_niche(value: &Self::CType) -> bool {
            A::is_niche(&value.0)
        }
    }

    impl<A: ReprC<CType: Copy>, B: Niche<CType: Copy>, N: LayoutKind> Niche for (A, B)
    where
        A: RustSpec<Niche = WithoutNiche>,
        B: RustSpec<Niche = WithNiche<N>>,
    {
        const NICHE: Self::CType = Tuple2(unsafe { core::mem::zeroed() }, B::NICHE);

        fn is_niche(value: &Self::CType) -> bool {
            B::is_niche(&value.1)
        }
    }
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: CType + Sized>> Niche for Vec<R> {
    const NICHE: Self::CType = <crate::vec::Vec<R> as Niche>::NICHE;

    fn is_niche(value: &Self::CType) -> bool {
        <crate::vec::Vec<R> as Niche>::is_niche(value)
    }
}

impl<T: ReprC<CType: Copy>> Niche for crate::option::Option<T> {
    const NICHE: Self::CType = COption::niche();

    fn is_niche(value: &Self::CType) -> bool {
        value.is_niche()
    }
}

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> Niche for crate::result::Result<T, E> {
    const NICHE: Self::CType = CResult::niche();

    fn is_niche(value: &Self::CType) -> bool {
        value.is_niche()
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

impl<R: ReprC<CType: CType + Copy>, E: ReprC<CType: CType + Copy>> Niche for Result<R, E>
where
    Self: ReprC<CType = CResult<R, E>>,
{
    const NICHE: Self::CType = CResult::niche();

    fn is_niche(value: &Self::CType) -> bool {
        value.is_niche()
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::string::String;
    use core::num::NonZero;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::{CType, Decode, Encode};

    #[test]
    fn nested_option_niche_family() {
        assert_impl_all!(Option<bool>:
            Niche<CType = COptionSome<bool>>,
            Decode<'static>,
            Encode,

        );
        assert_impl_all!(Option<Option<bool>>:
            Niche<CType = COptionSome<Option<bool>>>,
            Decode<'static>,
            Encode,

        );
        assert_impl_all!(Option<(u8, NonZero<u8>)>:
            ReprC<CType = COptionSome<(u8, NonZero<u8>)>>,
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
        assert!(!bool::is_niche(&CBool(0)));
        assert_eq!(
            unsafe { crate::decode::<Option<bool>>(crate::encode(None::<bool>)) },
            Some(None)
        );
        assert_eq!(
            unsafe { crate::decode::<Option<Ordering>>(crate::encode(None::<Ordering>)) },
            Some(None)
        );
        assert_eq!(
            unsafe { crate::decode::<Option<char>>(crate::encode(None::<char>)) },
            Some(None)
        );
        assert_eq!(
            unsafe {
                crate::decode::<Option<crate::option::Option<u32>>>(crate::encode(
                    None::<crate::option::Option<u32>>,
                ))
            },
            Some(None)
        );
        assert_eq!(
            unsafe { crate::decode::<Option<Option<bool>>>(crate::encode(None::<Option<bool>>)) },
            Some(None)
        );
        assert_eq!(
            unsafe { crate::decode::<Option<(u8, bool)>>(crate::encode(None::<(u8, bool)>)) },
            Some(None)
        );
        assert_eq!(
            unsafe { crate::decode::<Result<bool, ()>>(crate::encode(Err::<bool, ()>(()))) },
            Some(Err(()))
        );
    }

    #[test]
    fn reserved_pointer_and_nonzero_carriers_are_not_reflexive() {
        fn check<T: PartialEq + PartialOrd>(value: T) {
            assert!(!(value == value));
            assert!(value.partial_cmp(&value).is_none());
        }

        check(crate::NonZero(0u32));
        check(crate::NonZero(core::ptr::null_mut::<u32>()));
        check(CSoftRef::<u8>::null());
        check(CSoftRefMut::<u8, true>::null());
        check(CSoftWideRef::<[u8]>::null());
        check(CSoftWideRefMut::<[u8], true>::null());
        #[cfg(feature = "alloc")]
        {
            check(CBox::<u8>::null());
            check(CWideBox::<[u8]>::null());
            check(crate::vec::CVec::<u8>::null());
            check(crate::utf8::Utf8String::null());
        }

        assert!(crate::NonZero(1u32) == crate::NonZero(1u32));
        assert!(crate::NonZero(1u32) < crate::NonZero(2u32));
        assert_eq!(
            unsafe {
                crate::decode::<Option<core::num::NonZeroU32>>(crate::encode(
                    None::<core::num::NonZeroU32>,
                ))
            },
            Some(None)
        );
        #[cfg(feature = "alloc")]
        assert_eq!(
            unsafe { crate::decode::<Option<Box<u8>>>(crate::encode(None::<Box<u8>>)) },
            Some(None)
        );
        let value = 7u8;
        let reference = CSoftRef::<u8>::from_ref(&value);
        assert!(reference == reference);
        assert!(crate::NonZero::new(reference).is_none());
        check(crate::NonZero(reference));
    }

    #[test]
    fn niche_values() {
        assert!(<&bool>::is_niche(
            &crate::encode(None::<&bool>).into_carrier()
        ));
        assert!(<&mut bool>::is_niche(
            &co3::soft_encode(None::<&mut bool>, &mut Default::default()).into_carrier()
        ));

        #[cfg(feature = "alloc")]
        assert!(String::is_niche(
            &crate::encode(None::<String>).into_carrier()
        ));
        #[cfg(feature = "alloc")]
        assert!(<Box<str>>::is_niche(
            &crate::encode(None::<Box<str>>).into_carrier()
        ));

        assert!(<&str>::is_niche(
            &crate::encode(None::<&str>).into_carrier()
        ));

        #[cfg(feature = "alloc")]
        assert!(<&mut str>::is_niche(
            &crate::soft_encode(None::<&mut str>, &mut Default::default()).into_carrier()
        ));

        assert_eq!(
            core::ptr::null_mut(),
            crate::encode(None::<NonNull<u32>>).into_carrier().0
        );

        #[cfg(feature = "alloc")]
        assert!(<ManuallyDrop<String>>::is_niche(&ManuallyDrop::new(
            <String as Niche>::NICHE
        )));

        assert!(<ManuallyDrop<bool> as Niche>::is_niche(
            &crate::encode(None::<ManuallyDrop<bool>>).into_carrier()
        ));
    }
}
