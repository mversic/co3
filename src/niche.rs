//! Logic related to the conversion of [`Option<T>`] to and from FFI-compatible representation

#[cfg(feature = "alloc")]
use alloc::{boxed::Box, vec::Vec};
use core::ptr::NonNull;

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::boxed::{CBox, CBoxedSlice};
use crate::{
    CType, ReprC, assert_arr_has_non_zero_len,
    option::ReprCOption,
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
    pub trait Niche: ReprC<CType: Copy> + Sized {
        const NICHE_VALUE: Self::CType;
    }

    impl<R, C> Niche for &R
    where
        Self: ReprC<CType = *const C>,
    {
        const NICHE_VALUE: Self::CType = core::ptr::null();
    }
    impl<R, C> Niche for &R
    where
        Self: ReprC<CType = *mut C>,
    {
        const NICHE_VALUE: Self::CType = core::ptr::null_mut();
    }
    impl<R: ?Sized, C> Niche for &R
    where
        Self: ReprC<CType = CSlice<C>>,
    {
        const NICHE_VALUE: Self::CType = CSlice::NICHE_VALUE;
    }
    impl<R: ?Sized, C> Niche for &R
    where
        Self: ReprC<CType = CSliceMut<C>>,
    {
        const NICHE_VALUE: Self::CType = CSliceMut::NICHE_VALUE;
    }

    impl<R, C> Niche for &mut R
    where
        Self: ReprC<CType = *mut C>,
    {
        const NICHE_VALUE: Self::CType = core::ptr::null_mut();
    }
    impl<R: ?Sized, C> Niche for &mut R
    where
        Self: ReprC<CType = CSliceMut<C>>,
    {
        const NICHE_VALUE: Self::CType = CSliceMut::NICHE_VALUE;
    }

    #[cfg(feature = "alloc")]
    impl<R, C> Niche for Box<R>
    where
        Self: ReprC<CType = CBox<C>>,
    {
        const NICHE_VALUE: Self::CType = CBox::NICHE_VALUE;
    }
    #[cfg(feature = "alloc")]
    impl<R: ?Sized, C> Niche for Box<R>
    where
        Self: ReprC<CType = CBoxedSlice<C>>,
    {
        const NICHE_VALUE: Self::CType = CBoxedSlice::NICHE_VALUE;
    }

    impl<T, C> Niche for NonNull<T>
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
        const NICHE_VALUE: Self::CType = ReprCOption::NICHE_VALUE;
    }
    // TODO: Depends on: https://github.com/mversic/co3/issues/33
    impl Niche for Option<bool>
    where
        Self: ReprC<CType = <bool as ReprC>::CType>,
    {
        const NICHE_VALUE: Self::CType = 3;
    }
    impl Niche for Option<Option<bool>>
    where
        Self: ReprC<CType = <bool as ReprC>::CType>,
    {
        const NICHE_VALUE: Self::CType = 4;
    }
}

#[cfg(feature = "alloc")]
impl<R, C> Niche for Vec<R>
where
    Self: ReprC<CType = CBoxedSlice<C>>,
{
    const NICHE_VALUE: Self::CType = CBoxedSlice::NICHE_VALUE;
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
    const NICHE_VALUE: Self::CType = ReprCResult::NICHE_VALUE;
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
            Niche<CType = u8>,
            Decode<'static>,
            Encode,

        );
        assert_impl_all!(Option<Option<bool>>:
            Niche<CType = u8>,
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
        assert_eq!(core::ptr::null::<u8>(), crate::encode(None::<&bool>));
        assert_eq!(
            core::ptr::null::<u8>(),
            co3::soft_encode(None::<&mut bool>, &mut Default::default())
        );

        #[cfg(feature = "alloc")]
        assert_eq!(
            CBoxedSlice::<u8>::NICHE_VALUE,
            crate::encode(None::<String>)
        );
        #[cfg(feature = "alloc")]
        assert_eq!(
            CBoxedSlice::<u8>::NICHE_VALUE,
            crate::encode(None::<Box<str>>)
        );

        assert_eq!(CSlice::<u8>::NICHE_VALUE, crate::encode(None::<&str>));

        #[cfg(feature = "alloc")]
        assert_eq!(
            co3::slice::CSliceMut::<u8>::NICHE_VALUE,
            crate::soft_encode(None::<&mut str>, &mut Default::default())
        );

        assert_eq!(core::ptr::null_mut(), crate::encode(None::<NonNull<u32>>));

        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_eq!(
        //    CBoxedSlice::<u8>::NICHE_VALUE,
        //    crate::encode(None::<ManuallyDrop<String>>)
        //);

        //assert_eq!(2_u8, crate::encode(None::<ManuallyDrop<bool>>));
    }
}
