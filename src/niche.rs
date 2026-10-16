//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::ptr::NonNull;

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CBoxCell, CWideBox, CWideBoxCell};
use crate::{
    CType, ReprC, assert_arr_has_non_zero_len,
    option::ReprCOption,
    primitives::CBool,
    reference::{CRef, CRefMut},
    result::ReprCResult,
    slice::{CWideRef, CWideRefMut},
    wide::Wide,
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
    impl<R: ?Sized, C: Wide + ?Sized> Niche for &R
    where
        Self: ReprC<CType = CWideRef<C>>,
    {
        const NICHE: Self::CType = CWideRef::null();
    }
    impl<R: ?Sized, C: Wide + ?Sized> Niche for &R
    where
        Self: ReprC<CType = CWideRefMut<C, false>>,
    {
        const NICHE: Self::CType = CWideRefMut::null();
    }

    impl<R: ?Sized, C: CType> Niche for &mut R
    where
        Self: ReprC<CType = CRefMut<C, true>>,
    {
        const NICHE: Self::CType = CRefMut::null();
    }
    impl<R: ?Sized, C: Wide + ?Sized> Niche for &mut R
    where
        Self: ReprC<CType = CWideRefMut<C, true>>,
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
    impl<R: ?Sized, C: CType> Niche for Box<R>
    where
        Self: ReprC<CType = CBoxCell<C>>,
    {
        const NICHE: Self::CType = CBoxCell::null();
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C: Wide + ?Sized> Niche for Box<R>
    where
        Self: ReprC<CType = CWideBox<C>>,
    {
        const NICHE: Self::CType = CWideBox::null();
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C: Wide + ?Sized> Niche for Box<R>
    where
        Self: ReprC<CType = CWideBoxCell<C>>,
    {
        const NICHE: Self::CType = CWideBoxCell::null();
    }

    #[cfg(feature = "alloc")]
    impl<R, C: Wide + ?Sized> Niche for Vec<R>
    where
        Self: ReprC<CType = CWideBox<C>>,
    {
        const NICHE: Self::CType = CWideBox::null();
    }
    #[cfg(feature = "alloc")]
    impl<R, C: Wide + ?Sized> Niche for Vec<R>
    where
        Self: ReprC<CType = CWideBoxCell<C>>,
    {
        const NICHE: Self::CType = CWideBoxCell::null();
    }

    impl<T: ?Sized, C: CType> Niche for NonNull<T>
    where
        Self: ReprC<CType = *mut C>,
    {
        const NICHE: Self::CType = core::ptr::null_mut();
    }
    // TODO: Support ?Sized
    //impl<R: ?C> Niche for NonNull<R>
    //where
    //    Self: ReprC<CType = CWideBox<C>>,
    //{
    //    const NICHE: Self::CType = CWideBox::none();
    //}

    impl<R, C: CType + Copy> Niche for Option<R>
    where
        Self: ReprC<CType = ReprCOption<C>>,
    {
        const NICHE: Self::CType = ReprCOption::niche();
    }
    // TODO: Depends on: https://github.com/mversic/co3/issues/33
    impl Niche for Option<bool>
    where
        Self: ReprC<CType = <bool as ReprC>::CType>,
    {
        const NICHE: Self::CType = CBool(3);
    }
    impl Niche for Option<Option<bool>>
    where
        Self: ReprC<CType = <bool as ReprC>::CType>,
    {
        const NICHE: Self::CType = CBool(4);
    }
}

impl<R: Niche, const N: usize> Niche for [R; N]
where
    Self: ReprC<CType = [R::CType; N]>,
{
    const NICHE: Self::CType = {
        assert_arr_has_non_zero_len::<N>();
        [R::NICHE; N]
    };
}

impl<R, E, C: CType + Copy, D: CType + Copy> Niche for Result<R, E>
where
    Self: ReprC<CType = ReprCResult<C, D>>,
{
    const NICHE: Self::CType = ReprCResult::niche();
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::string::String;
    use core::num::NonZero;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::{CType, Decode, Encode, tuple::ReprCTuple2};

    #[test]
    fn nested_option_niche_family() {
        assert_impl_all!(Option<bool>:
            Niche<CType = CBool>,
            Decode<'static>,
            Encode,

        );
        assert_impl_all!(Option<Option<bool>>:
            Niche<CType = CBool>,
            Decode<'static>,
            Encode,

        );
        assert_impl_all!(Option<(u8, NonZero<u8>)>:
            ReprC<CType = ReprCTuple2<u8, u8>>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(Option<bool>: CType);
        assert_not_impl_any!(Option<Option<bool>>: CType);
    }

    #[test]
    fn niche_values() {
        assert_eq!(CRef::<CBool>::null(), crate::encode(None::<&bool>));
        assert_eq!(
            <&mut bool as Niche>::NICHE,
            co3::soft_encode(None::<&mut bool>, &mut Default::default())
        );

        #[cfg(feature = "alloc")]
        assert_eq!(<String as Niche>::NICHE, crate::encode(None::<String>));
        #[cfg(feature = "alloc")]
        assert_eq!(<Box<str> as Niche>::NICHE, crate::encode(None::<Box<str>>));

        assert_eq!(<&str as Niche>::NICHE, crate::encode(None::<&str>));

        #[cfg(feature = "alloc")]
        assert_eq!(
            <&mut str as Niche>::NICHE,
            crate::soft_encode(None::<&mut str>, &mut Default::default())
        );

        assert_eq!(core::ptr::null_mut(), crate::encode(None::<NonNull<u32>>));

        #[cfg(feature = "alloc")]
        assert_eq!(
            <core::mem::ManuallyDrop<String> as Niche>::NICHE,
            core::mem::ManuallyDrop::new(<String as Niche>::NICHE)
        );

        assert_eq!(
            <core::mem::ManuallyDrop<bool> as Niche>::NICHE,
            crate::encode(None::<core::mem::ManuallyDrop<bool>>)
        );
    }
}
