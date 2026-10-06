//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::ptr::NonNull;

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CBoxCell, CBoxedSlice, CBoxedSliceCell};
use crate::{
    CType, ReprC, assert_arr_has_non_zero_len,
    option::ReprCOption,
    primitives::CBool,
    reference::CRefMut,
    result::ReprCResult,
    slice::{CSlice, CSliceMut},
};

disjoint_impls! {
    /// Type that has a trap representation that can be used as a niche value.
    ///
    /// # Example
    ///
    /// [`Option<bool>`]     - will be serilized into one byte
    /// [`Option<*const T>`] - will take the size of the pointer
    pub trait Niche: ReprC<CType: Copy> {
        const NICHE_VALUE: Self::CType;
    }

    impl<R: ?Sized, C> Niche for &R
    where
        Self: ReprC<CType = *const C>,
    {
        const NICHE_VALUE: Self::CType = core::ptr::null();
    }
    impl<R: ?Sized, C> Niche for &R
    where
        Self: ReprC<CType = CRefMut<C, false>>,
    {
        const NICHE_VALUE: Self::CType = CRefMut::<C, false>::NICHE;
    }
    impl<R: ?Sized, C> Niche for &R
    where
        Self: ReprC<CType = CSlice<C>>,
    {
        const NICHE_VALUE: Self::CType = CSlice::NICHE;
    }
    impl<R: ?Sized, C> Niche for &R
    where
        Self: ReprC<CType = CSliceMut<C, false>>,
    {
        const NICHE_VALUE: Self::CType = CSliceMut::<C, false>::NICHE;
    }

    impl<R: ?Sized, C> Niche for &mut R
    where
        Self: ReprC<CType = CRefMut<C>>,
    {
        const NICHE_VALUE: Self::CType = CRefMut::NICHE;
    }
    impl<R: ?Sized, C> Niche for &mut R
    where
        Self: ReprC<CType = CSliceMut<C>>,
    {
        const NICHE_VALUE: Self::CType = CSliceMut::<C>::NICHE;
    }

    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C> Niche for Box<R>
    where
        Self: ReprC<CType = CBox<C>>,
    {
        const NICHE_VALUE: Self::CType = CBox::NICHE;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C> Niche for Box<R>
    where
        Self: ReprC<CType = CBoxCell<C>>,
    {
        const NICHE_VALUE: Self::CType = CBoxCell::NICHE;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C> Niche for Box<R>
    where
        Self: ReprC<CType = CBoxedSlice<C>>,
    {
        const NICHE_VALUE: Self::CType = CBoxedSlice::NICHE;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C> Niche for Box<R>
    where
        Self: ReprC<CType = CBoxedSliceCell<C>>,
    {
        const NICHE_VALUE: Self::CType = CBoxedSliceCell::NICHE;
    }

    #[cfg(feature = "alloc")]
    impl<R, C> Niche for Vec<R>
    where
        Self: ReprC<CType = CBoxedSlice<C>>,
    {
        const NICHE_VALUE: Self::CType = CBoxedSlice::NICHE;
    }
    #[cfg(feature = "alloc")]
    impl<R, C> Niche for Vec<R>
    where
        Self: ReprC<CType = CBoxedSliceCell<C>>,
    {
        const NICHE_VALUE: Self::CType = CBoxedSliceCell::NICHE;
    }

    impl<T: ?Sized, C> Niche for NonNull<T>
    where
        Self: ReprC<CType = *mut C>,
    {
        const NICHE_VALUE: Self::CType = core::ptr::null_mut();
    }
    // TODO: Support ?Sized
    //impl<R: ?C> Niche for NonNull<R>
    //where
    //    Self: ReprC<CType = CBoxedSlice<C>>,
    //{
    //    const NICHE_VALUE: Self::CType = CBoxedSlice::none();
    //}

    impl<R, C: CType + Copy> Niche for Option<R>
    where
        Self: ReprC<CType = ReprCOption<C>>,
    {
        const NICHE_VALUE: Self::CType = ReprCOption::NICHE;
    }
    // TODO: Depends on: https://github.com/mversic/co3/issues/33
    impl Niche for Option<bool>
    where
        Self: ReprC<CType = <bool as ReprC>::CType>,
    {
        const NICHE_VALUE: Self::CType = CBool::from_raw(3);
    }
    impl Niche for Option<Option<bool>>
    where
        Self: ReprC<CType = <bool as ReprC>::CType>,
    {
        const NICHE_VALUE: Self::CType = CBool::from_raw(4);
    }
}

impl<R: Niche, const N: usize> Niche for [R; N]
where
    Self: ReprC<CType = [R::CType; N]>,
{
    const NICHE_VALUE: Self::CType = {
        assert_arr_has_non_zero_len::<N>();
        [R::NICHE_VALUE; N]
    };
}

impl<R, E, C: CType + Copy, D: CType + Copy> Niche for Result<R, E>
where
    Self: ReprC<CType = ReprCResult<C, D>>,
{
    const NICHE_VALUE: Self::CType = ReprCResult::NICHE;
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "alloc")]
    use alloc::string::String;
    use core::num::NonZero;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    use crate::{CType, Decode, Encode, slice::CSlice, tuple::ReprCTuple2};

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
        assert_eq!(core::ptr::null::<CBool>(), crate::encode(None::<&bool>));
        assert_eq!(
            CRefMut::<CBool>::NICHE,
            co3::soft_encode(None::<&mut bool>, &mut Default::default())
        );

        #[cfg(feature = "alloc")]
        assert_eq!(CBoxedSlice::<u8>::NICHE, crate::encode(None::<String>));
        #[cfg(feature = "alloc")]
        assert_eq!(CBoxedSlice::<u8>::NICHE, crate::encode(None::<Box<str>>));

        assert_eq!(CSlice::<u8>::NICHE, crate::encode(None::<&str>));

        #[cfg(feature = "alloc")]
        assert_eq!(
            co3::slice::CSliceMut::<u8>::NICHE,
            crate::soft_encode(None::<&mut str>, &mut Default::default())
        );

        assert_eq!(core::ptr::null_mut(), crate::encode(None::<NonNull<u32>>));

        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_eq!(
        //    CBoxedSlice::<u8>::NICHE,
        //    crate::encode(None::<ManuallyDrop<String>>)
        //);

        //assert_eq!(2_u8, crate::encode(None::<ManuallyDrop<bool>>));
    }
}
