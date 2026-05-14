#[cfg(feature = "alloc")]
use alloc_crate::boxed::Box;
use core::mem::ManuallyDrop;

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::boxed::CBox;
use crate::{
    ReprC, SizeFamily, Wide, assert_arr_has_non_zero_len,
    ir::{ReprFamily, Robust, Transmuted},
    niche::{NicheFamily, StableNiche, WithStableNiche},
    size::MetaSized,
};

disjoint_impls! {
    /// Type that can be **safely transmuted** into another type.
    ///
    /// # Safety
    ///
    /// - `Self` and `Self::Target` must be mutually transmutable (this includes [`Drop`] semantics)
    /// - `Self::is_valid` must not return false positives, i.e. return `true` for trap representations
    pub unsafe trait CheckedTransmute {
        /// Type that [`Self`] can be transmuted into
        type Target: ?Sized;

        /// Called when transmuting [`Self::Target`] back into [`Self`] to check for trap representations.
        ///
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::Target) -> bool;
    }

    unsafe impl<R: ReprFamily<Kind = Transmuted> + CheckedTransmute<Target: Sized>> CheckedTransmute for [R] {
        type Target = [R::Target];

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            target.iter().all(R::is_valid)
        }
    }

    unsafe impl<R: ReprFamily<Kind = Robust>> CheckedTransmute for &R {
        type Target = *const R;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<'a, R: ReprFamily<Kind = Transmuted> + CheckedTransmute> CheckedTransmute for &'a R {
        type Target = &'a R::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }

    unsafe impl<R: ReprFamily<Kind = Robust>> CheckedTransmute for &mut R {
        type Target = *mut R;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<'a, R: ReprFamily<Kind = Transmuted> + CheckedTransmute> CheckedTransmute for &'a mut R {
        type Target = &'a mut R::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprFamily<Kind = Robust>> CheckedTransmute for Box<R> {
        type Target = CBox<R>;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_none()
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprFamily<Kind = Transmuted> + CheckedTransmute> CheckedTransmute for Box<R> {
        type Target = Box<R::Target>;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }

    unsafe impl<R: NicheFamily<Kind = WithStableNiche> + StableNiche> CheckedTransmute for Option<R>
    where
        R: CheckedTransmute<Target: ReprFamily<Kind = Robust>>,
    {
        type Target = R::Target;

        #[inline(always)]
        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }
    unsafe impl<R: NicheFamily<Kind = WithStableNiche> + StableNiche> CheckedTransmute for Option<R>
    where
        R: CheckedTransmute<Target: ReprFamily<Kind = Transmuted> + Sized>,
    {
        type Target = Option<R::Target>;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            target.as_ref().is_none_or(R::is_valid)
        }
    }
}

disjoint_impls! {
    /// Type that can compresses the chain of transmutations done via [`CheckedTransmute`].
    ///
    /// # Safety
    ///
    /// - check [`CheckedTransmute`]
    pub unsafe trait FlatTransmute {
        type Target: ReprC + ?Sized;

        /// Called when transmuting [`Self::Target`] back into [`Self`] to check for trap representations.
        ///
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::Target) -> bool;
    }

    unsafe impl<R: ReprC + ?Sized> FlatTransmute for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type Target = Self;

        #[inline(always)]
        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }

    unsafe impl<R: CheckedTransmute, K> FlatTransmute for R
    where
        Self: ReprFamily<Kind = Transmuted> + SizeFamily<Kind = crate::size::Sized<K>>,
        // TODO: ReprC bound shouldn't be required
        <R as CheckedTransmute>::Target: FlatTransmute<Target: ReprFamily<Kind = Robust> + ReprC> + Sized,
    {
        type Target = <R::Target as FlatTransmute>::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            if !<R::Target as FlatTransmute>::is_valid(target) {
                return false;
            }

            let target_ptr = core::ptr::from_ref(target).cast::<R::Target>();
            <R as CheckedTransmute>::is_valid(unsafe { &*target_ptr })
        }
    }
    unsafe impl<R: CheckedTransmute + ?Sized, K> FlatTransmute for R
    where
        Self: ReprFamily<Kind = Transmuted> + SizeFamily<Kind = MetaSized<K>>,
        // TODO: ReprC bound shouldn't be required
        <R as CheckedTransmute>::Target: FlatTransmute<Target: ReprFamily<Kind = Robust> + Wide + ReprC>,
        <R as CheckedTransmute>::Target: Wide<
            Metadata = <<<R as CheckedTransmute>::Target as FlatTransmute>::Target as Wide>::Metadata,
        >,
    {
        type Target = <<R as CheckedTransmute>::Target as FlatTransmute>::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            if !R::Target::is_valid(target) {
                return false;
            }

            let target =
                unsafe { R::Target::from_raw_parts(target.as_ptr().cast(), target.metadata()) };

            R::is_valid(target)
        }
    }
}

unsafe impl<R: CheckedTransmute<Target: Sized>, const N: usize> CheckedTransmute for [R; N] {
    type Target = [R::Target; N];

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        assert_arr_has_non_zero_len::<N>();
        target.iter().all(R::is_valid)
    }
}

#[repr(C)]
union TransmuteHelper<R: CheckedTransmute<Target: Sized>> {
    source: ManuallyDrop<R>,
    target: ManuallyDrop<R::Target>,
}

pub(crate) fn transmute_into_target<R: CheckedTransmute<Target: Sized>>(source: R) -> R::Target {
    assert_size_and_allignment_match::<R>();

    let transmute_helper = TransmuteHelper {
        source: ManuallyDrop::new(source),
    };

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    ManuallyDrop::into_inner(unsafe { transmute_helper.target })
}
pub(crate) fn transmute_from_target<R: CheckedTransmute<Target: Sized>>(
    source: R::Target,
) -> Option<R> {
    assert_size_and_allignment_match::<R>();

    if !R::is_valid(&source) {
        return None;
    }

    let transmute_helper = TransmuteHelper {
        target: ManuallyDrop::new(source),
    };

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Some(ManuallyDrop::into_inner(unsafe { transmute_helper.source }))
}

pub(super) fn transmute_into_target_ref_dst<R>(source: &R) -> &R::Target
where
    R: Wide<Metadata = <R::Target as Wide>::Metadata>,
    R: CheckedTransmute + ?Sized,
    R::Target: Wide,
{
    let len = source.metadata();

    let (ptr, len) = (source.as_ptr().cast(), len);
    unsafe { R::Target::from_raw_parts(ptr, len) }
}

pub(super) fn transmute_into_target_dst_mut<R>(source: &mut R) -> &mut R::Target
where
    R: Wide<Metadata = <R::Target as Wide>::Metadata>,
    R: CheckedTransmute + ?Sized,
    R::Target: Wide,
{
    let len = source.metadata();

    let (ptr, len) = (source.as_mut_ptr().cast(), len);
    unsafe { R::Target::from_raw_parts_mut(ptr, len) }
}
#[cfg(feature = "alloc")]
pub(super) fn transmute_into_target_boxed_dst<R>(source: Box<R>) -> Box<R::Target>
where
    R: Wide<Metadata = <R::Target as Wide>::Metadata>,
    R: CheckedTransmute + ?Sized,
    R::Target: Wide,
{
    let len = source.metadata();

    let data = R::into_non_null(source).cast();
    unsafe { R::Target::from_non_null(data, len) }
}

pub(super) fn transmute_from_target_ref_dst<R>(source: &R::Target) -> Option<&R>
where
    R: Wide<Metadata = <R::Target as Wide>::Metadata>,
    R: CheckedTransmute + ?Sized,
    R::Target: Wide,
{
    let len = source.metadata();

    if !R::is_valid(source) {
        return None;
    }

    let (ptr, len) = (source.as_ptr().cast(), len);
    Some(unsafe { R::from_raw_parts(ptr, len) })
}
pub(super) fn transmute_from_target_dst_mut<R>(source: &mut R::Target) -> Option<&mut R>
where
    R: Wide<Metadata = <R::Target as Wide>::Metadata>,
    R: CheckedTransmute + ?Sized,
    R::Target: Wide,
{
    let len = source.metadata();

    if !R::is_valid(source) {
        return None;
    }

    let (ptr, len) = (source.as_mut_ptr().cast(), len);
    Some(unsafe { R::from_raw_parts_mut(ptr, len) })
}
#[cfg(feature = "alloc")]
pub(super) fn transmute_from_target_boxed_dst<R>(source: Box<R::Target>) -> Option<Box<R>>
where
    R: Wide<Metadata = <R::Target as Wide>::Metadata>,
    R: CheckedTransmute + ?Sized,
    R::Target: Wide,
{
    let len = source.metadata();

    if !R::is_valid(&source) {
        return None;
    }

    let data = R::Target::into_non_null(source).cast();
    Some(unsafe { R::from_non_null(data, len) })
}

fn assert_size_and_allignment_match<R: CheckedTransmute<Target: Sized>>() {
    const {
        debug_assert!(core::mem::size_of::<R>() == core::mem::size_of::<R::Target>());
        debug_assert!(core::mem::align_of::<R>() == core::mem::align_of::<R::Target>());
    };
}

#[cfg(test)]
mod tests {
    use alloc_crate::vec::Vec;

    use static_assertions::assert_impl_all;

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::CBoxedSlice;
    use crate::{
        Decode, Encode, ExternC,
        ir::{ReprFamily, Transmuted},
        niche::{Niche, NicheFamily, StableNiche, WithCustomNiche, WithStableNiche, WithoutNiche},
        slice::{CSlice, CSliceMut},
    };

    #[test]
    fn transparent_type() {
        assert_impl_all!(bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<bool>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<u8>>,
            // FIXME:
            //SoftDecodeView<'static>,
            // Decode,
            Encode,
        );
        assert_impl_all!(&[bool]:
            ReprFamily<Kind = &'static [bool]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [bool]:
            ReprFamily<Kind = &'static mut [bool]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[bool]>:
            ReprFamily<Kind = Box<[bool]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<bool>:
            ReprFamily<Kind = Vec<bool>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([bool; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<bool>:
            ReprFamily<Kind = Option<bool>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = u8>,
            Decode<'static>,
            Encode,
        );

        // FIXME:
        //assert_impl_all!(&mut bool: SoftEncode);
        //assert_impl_all!(&mut [bool]: SoftEncode);
        //assert_not_impl_any!(&mut bool: SoftEncode);
        //assert_not_impl_any!(&mut [bool]: SoftEncode);
    }

    #[test]
    fn robust_ref() {
        assert_impl_all!(&&u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *const u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<*const u8>>,
            // FIXME:
            //Decode<'static>,
            //SoftDecodeView<'static>,
            Encode,
        );
        assert_impl_all!(&[&u8]:
            ReprFamily<Kind = &'static [&'static u8]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&u8]:
            ReprFamily<Kind = &'static mut [&'static u8]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&u8]>:
            ReprFamily<Kind = Box<[&'static u8]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            // FIXME:
            //Decode<'static>,
            //SoftDecodeView<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&u8>:
            ReprFamily<Kind = Vec<&'static u8>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&u8; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*const u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&u8>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *const u8>,
            Decode<'static>,
            Encode,
        );

        // FIXME:
        //assert_impl_all!(&mut &u8: SoftEncode);
        //assert_impl_all!(&mut [&u8]: SoftEncode);
        //assert_not_impl_any!(&mut &u8: SoftEncode);
        //assert_not_impl_any!(&mut [&u8]: SoftEncode);
    }

    #[test]
    fn transparent_ref() {
        assert_impl_all!(&&bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *const u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *const u8>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&bool>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<*const u8>>,
            // FIXME:
            //Decode<'static>,
            //SoftDecodeView<'static>,
            Encode,
        );
        assert_impl_all!(&[&bool]:
            ReprFamily<Kind = &'static [&'static bool]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&bool]:
            ReprFamily<Kind = &'static mut [&'static bool]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&bool]>:
            ReprFamily<Kind = Box<[&'static bool]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            // FIXME:
            //Decode<'static>,
            //SoftDecodeView<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&bool>:
            ReprFamily<Kind = Vec<&'static bool>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&bool; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*const u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&bool>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *const u8>,
            Decode<'static>,
            Encode,
        );

        // FIXME:
        //assert_impl_all!(&mut &bool: SoftEncode);
        //assert_impl_all!(&mut [&bool]: SoftEncode);
        //assert_not_impl_any!(&mut &bool: SoftEncode);
        //assert_not_impl_any!(&mut [&bool]: SoftEncode);
    }

    #[test]
    fn robust_ref_mut() {
        assert_impl_all!(&&mut u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *mut u8>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut u8>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<*mut u8>>,
            // FIXME:
            //Decode<'static>,
            //SoftDecodeView<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut u8]:
            ReprFamily<Kind = &'static [&'static mut u8]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut u8]:
            ReprFamily<Kind = &'static mut [&'static mut u8]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut u8]>:
            ReprFamily<Kind = Box<[&'static mut u8]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            // FIXME:
            //Decode<'static>,
            //SoftDecodeView<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut u8>:
            ReprFamily<Kind = Vec<&'static mut u8>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!([&mut u8; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*mut u8; 2]>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(Option<&mut u8>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *mut u8>,
            Decode<'static>,
            Encode,
        );

        // FIXME:
        //assert_impl_all!(&mut &mut u8: SoftEncode);
        //assert_impl_all!(&mut [&mut u8]: SoftEncode);
        //assert_not_impl_any!(&mut &mut u8: SoftEncode);
        //assert_not_impl_any!(&mut [&mut u8]: SoftEncode);
    }

    #[test]
    fn transparent_ref_mut() {
        assert_impl_all!(&&mut bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *mut u8>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut &mut bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *mut u8>,
            Decode<'static>,
            Encode
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<&mut bool>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = CBox<*mut u8>>,
            // FIXME:
            //Decode<'static>,
            //SoftDecodeView<'static>,
            Encode,
        );
        assert_impl_all!(&[&mut bool]:
            ReprFamily<Kind = &'static [&'static mut bool]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut bool]:
            ReprFamily<Kind = &'static mut [&'static mut bool]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut bool]>:
            ReprFamily<Kind = Box<[&'static mut bool]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            // FIXME:
            //Decode<'static>,
            //SoftDecodeView<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut bool>:
            ReprFamily<Kind = Vec<&'static mut bool>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!([&mut bool; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*mut u8; 2]>,
            Decode<'static>,
            Encode
        );
        assert_impl_all!(Option<&mut bool>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *mut u8>,
            Decode<'static>,
            Encode
        );

        // FIXME:
        //assert_impl_all!(&mut &mut bool: SoftEncode);

        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<&mut bool>: SoftEncode);
        //assert_impl_all!(&mut [&mut bool]: SoftEncode);
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<[&mut bool]>: SoftEncode);
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Vec<&mut bool>: SoftEncode);
        //assert_impl_all!([&mut bool; 2]: SoftEncode);
        //assert_impl_all!(Option<&mut bool>: SoftEncode);
        //assert_not_impl_any!(&mut &mut bool: SoftEncode);
        //#[cfg(feature = "alloc")]
        //assert_not_impl_any!(Box<&mut bool>: SoftEncode);
        //assert_not_impl_any!(&mut [&mut bool]: SoftEncode);
        //#[cfg(feature = "alloc")]
        //assert_not_impl_any!(Box<[&mut bool]>: SoftEncode);
        //#[cfg(feature = "alloc")]
        //assert_not_impl_any!(Vec<[&mut bool]>: SoftEncode);
        //assert_not_impl_any!([&mut bool; 2]: SoftEncode);
        //assert_not_impl_any!(Option<[&mut bool]>: SoftEncode);
    }
}
