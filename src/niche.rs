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

#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CWideBox};
use crate::{
    CType, NeedsDrop, ReprC,
    option::{COption, COptionSome},
    primitives::Bool,
    reference::{CRef, CRefMut},
    result::CResult,
    slice::{CWideRef, CWideRefMut},
    tuple::Tuple2,
    wide::{Wide, WideHeader},
};

disjoint_impls! {
    /// Type that has a trap representation that can be used as a niche value.
    ///
    /// # Example
    ///
    /// [`Option<bool>`]     - will be serilized into one byte
    /// [`Option<*const T>`] - will take the size of the pointer
    pub trait Niche: ReprC<CType: Copy> {
        const NICHE: Self::CType;
    }

    impl<R: ?Sized, C: CType> Niche for &R
    where
        Self: ReprC<CType = CRef<C>>,
    {
        const NICHE: Self::CType = CRef::null();
    }
    impl<R: ?Sized, C: CType> Niche for &R
    where
        Self: ReprC<CType = CRefMut<C, false>>,
    {
        const NICHE: Self::CType = CRefMut::null();
    }
    impl<R: ?Sized, C: Wide<Header: WideHeader<Data: CType>> + ?Sized> Niche for &R
    where
        Self: ReprC<CType = CWideRef<C>>,
        <C as Wide>::Metadata: CType,
    {
        const NICHE: Self::CType = CWideRef::null();
    }
    impl<R: ?Sized, C: Wide<Header: WideHeader<Data: CType>> + ?Sized> Niche for &R
    where
        Self: ReprC<CType = CWideRefMut<C, false>>,
        <C as Wide>::Metadata: CType,
    {
        const NICHE: Self::CType = CWideRefMut::null();
    }

    impl<R: ?Sized, C: CType> Niche for &mut R
    where
        Self: ReprC<CType = CRefMut<C, true>>,
    {
        const NICHE: Self::CType = CRefMut::null();
    }
    impl<R: ?Sized, C: Wide<Header: WideHeader<Data: CType>> + ?Sized> Niche for &mut R
    where
        Self: ReprC<CType = CWideRefMut<C, true>>,
        <C as Wide>::Metadata: CType,
    {
        const NICHE: Self::CType = CWideRefMut::null();
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C: CType> Niche for Box<R>
    where
        Self: ReprC<CType = CBox<C>>,
    {
        const NICHE: Self::CType = CBox::null();
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C: Wide<Header: WideHeader<Data: CType>> + ?Sized> Niche for Box<R>
    where
        Self: ReprC<CType = CWideBox<C>>,
        <C as Wide>::Metadata: CType,
    {
        const NICHE: Self::CType = CWideBox::null();
    }


    impl<T: ?Sized, C: CType> Niche for NonNull<T>
    where
        Self: ReprC<CType = crate::NonZero<*mut C>>,
    {
        const NICHE: Self::CType = crate::NonZero(core::ptr::null_mut());
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
    }
    // TODO: Depends on: https://github.com/mversic/co3/issues/33
    impl Niche for Option<bool>
    where
        Self: ReprC<CType = COptionSome<bool>>,
    {
        const NICHE: Self::CType = COptionSome::from_carrier(Bool(3));
    }
    impl Niche for Option<Option<bool>>
    where
        Self: ReprC<CType = COptionSome<Option<bool>>>,
    {
        const NICHE: Self::CType = COptionSome::from_carrier(COptionSome::from_carrier(Bool(4)));
    }

    impl<R: Niche> Niche for ManuallyDrop<R>
    where
        R: RustSpec<Drop = NoDrop>,
    {
        const NICHE: Self::CType = R::NICHE;
    }
    impl<R: Niche> Niche for ManuallyDrop<R>
    where
        R: RustSpec<Drop: NeedsDrop>,
    {
        const NICHE: Self::CType = ManuallyDrop::new(R::NICHE);
    }

    impl<A: Niche<CType: Copy>, B: ReprC<CType: Copy>, N: LayoutKind> Niche for (A, B)
    where
        A: RustSpec<Niche = WithNiche<N>>,
    {
        const NICHE: Self::CType = Tuple2(A::NICHE, unsafe { core::mem::zeroed() });
    }

    impl<A: ReprC<CType: Copy>, B: Niche<CType: Copy>, N: LayoutKind> Niche for (A, B)
    where
        A: RustSpec<Niche = WithoutNiche>,
        B: RustSpec<Niche = WithNiche<N>>,
    {
        const NICHE: Self::CType = Tuple2(unsafe { core::mem::zeroed() }, B::NICHE);
    }
}

#[cfg(feature = "alloc")]
impl<R: ReprC<CType: CType + Sized>> Niche for Vec<R> {
    const NICHE: Self::CType = <crate::vec::Vec<R> as Niche>::NICHE;
}

impl<T: ReprC<CType: Copy>> Niche for crate::option::Option<T> {
    const NICHE: Self::CType = COption::niche();
}

impl<T: ReprC<CType: Copy>, E: ReprC<CType: Copy>> Niche for crate::result::Result<T, E> {
    const NICHE: Self::CType = CResult::niche();
}

impl<R: Niche, const N: usize> Niche for [R; N] {
    const NICHE: Self::CType = { [R::NICHE; N] };
}

impl<R: ReprC<CType: CType + Copy>, E: ReprC<CType: CType + Copy>> Niche for Result<R, E>
where
    Self: ReprC<CType = CResult<R, E>>,
{
    const NICHE: Self::CType = CResult::niche();
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
    fn niche_values() {
        assert_eq!(
            CRef::<Bool>::null(),
            crate::encode(None::<&bool>).into_carrier()
        );
        assert_eq!(
            <&mut bool as Niche>::NICHE,
            co3::soft_encode(None::<&mut bool>, &mut Default::default()).into_carrier()
        );

        #[cfg(feature = "alloc")]
        assert_eq!(
            <String as Niche>::NICHE,
            crate::encode(None::<String>).into_carrier()
        );
        #[cfg(feature = "alloc")]
        assert_eq!(
            <Box<str> as Niche>::NICHE,
            crate::encode(None::<Box<str>>).into_carrier()
        );

        assert_eq!(
            <&str as Niche>::NICHE,
            crate::encode(None::<&str>).into_carrier()
        );

        #[cfg(feature = "alloc")]
        assert_eq!(
            <&mut str as Niche>::NICHE,
            crate::soft_encode(None::<&mut str>, &mut Default::default()).into_carrier()
        );

        assert_eq!(
            core::ptr::null_mut(),
            crate::encode(None::<NonNull<u32>>).into_carrier().0
        );

        #[cfg(feature = "alloc")]
        assert_eq!(
            <core::mem::ManuallyDrop<String> as Niche>::NICHE,
            core::mem::ManuallyDrop::new(<String as Niche>::NICHE)
        );

        assert_eq!(
            <core::mem::ManuallyDrop<bool> as Niche>::NICHE,
            crate::encode(None::<core::mem::ManuallyDrop<bool>>).into_carrier()
        );
    }
}
