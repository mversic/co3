use core::mem::ManuallyDrop;

use alloc::boxed::Box;
#[cfg(feature = "owned_as_ref")]
#[cfg(feature = "owned_types")]
use alloc::vec::Vec;
use disjoint_impls::disjoint_impls;

use crate::{
    ExternC, ReprC, assert_arr_has_non_zero_len,
    ir::{Opaque, ReprFamily, Robust, Transparent},
    niche::StableNiche,
};

disjoint_impls! {
    /// # Safety
    ///
    // TODO: Write safety comment
    pub unsafe trait Encodable {}

    unsafe impl<R: ReprFamily<Kind = Robust>> Encodable for R {}

    unsafe impl<R> Encodable for &R where Self: ReprFamily<Kind = Transparent> {}
    unsafe impl<R: ReprC> Encodable for &mut R where Self: ReprFamily<Kind = Transparent> {}
    // WARN: Since `Box<&mut R>` is mapped to `*mut *mut R` this can be disputed in the case of
    // no ownership transfer where Box's invariant can be violated by the caller by NULLing the
    // inner pointer. However, because the box is immediately dropped following the function call,
    // we deem it ok as it would most likely lead to a catastrophic segfault, not a silent UB.
    unsafe impl<R: crate::Encode> Encodable for Box<R> where Self: ReprFamily<Kind = Transparent> {}
    unsafe impl<R: crate::Encode> Encodable for Option<R> where Self: ReprFamily<Kind = Transparent> {}
}

disjoint_impls! {
    /// Marker trait for a type that can be **safely transmuted** into another type.
    ///
    /// # Safety
    ///
    /// - `Self` and `Self::Target` must be mutually transmutable (this includes [`Drop`] semantics)
    /// - `Self::is_valid` must not return false positives, i.e. return `true` for trap representations
    pub unsafe trait CheckedTransmute {
        /// Type that [`Self`] can be transmuted into
        type Target;

        /// Called when transmuting [`Self::Target`] back into [`Self`] to check for trap representations.
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::Target) -> bool;
    }

    unsafe impl<'a, R: ReprFamily<Kind = Transparent> + CheckedTransmute> CheckedTransmute for &'a R {
        type Target = &'a R::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }
    unsafe impl<'a, R: ReprFamily<Kind = Box<Robust>> + CheckedTransmute> CheckedTransmute for &'a R {
        type Target = &'a R::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }
    unsafe impl<R: ReprFamily<Kind = Robust> + ReprC> CheckedTransmute for &R {
        type Target = *const R;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<R: ReprFamily<Kind = Opaque>> CheckedTransmute for &R {
        type Target = *const R;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }

    unsafe impl<'a, R: ReprFamily<Kind = Transparent> + CheckedTransmute> CheckedTransmute for &'a mut R {
        type Target = &'a mut R::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }
    unsafe impl<'a, R: ReprFamily<Kind = Box<Robust>> + CheckedTransmute> CheckedTransmute for &'a mut R {
        type Target = &'a mut R::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }
    unsafe impl<R: ReprFamily<Kind = Robust> + ReprC> CheckedTransmute for &mut R {
        type Target = *mut R;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<R: ReprFamily<Kind = Opaque>> CheckedTransmute for &mut R {
        type Target = *mut R;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }

    unsafe impl<R: ReprFamily<Kind = Transparent> + CheckedTransmute> CheckedTransmute for Box<R> {
        type Target = Box<R::Target>;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }
    #[cfg(feature = "owned_types")]
    unsafe impl<R: ReprFamily<Kind = Box<Robust>> + CheckedTransmute> CheckedTransmute for Box<R> {
        type Target = Box<R::Target>;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }
    #[cfg(feature = "owned_types")]
    unsafe impl<R: ReprFamily<Kind = Robust> + ReprC> CheckedTransmute for Box<R> {
        type Target = *mut R;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<R: ReprFamily<Kind = Opaque>> CheckedTransmute for Box<R> {
        type Target = *mut R;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }

    unsafe impl<R: CheckedTransmute<Target: ReprFamily<Kind = Transparent>> + StableNiche> CheckedTransmute for Option<R> {
        type Target = Option<R::Target>;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            target.as_ref().is_none_or(R::is_valid)
        }
    }
    unsafe impl<R: CheckedTransmute<Target: ReprFamily<Kind = Robust> + ReprC> + StableNiche> CheckedTransmute for Option<R> {
        type Target = R::Target;

        #[inline(always)]
        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }
}

disjoint_impls! {
    /// Marker trait for a type that can be **safely transmuted** into another [`ReprC`] type.
    ///
    /// This trait compresses the chain of transmutations done via [`CheckedTransmute`]
    ///
    /// # Safety
    ///
    /// - `Self` and `Self::CType` must be mutually transmutable (this includes [`Drop`] semantics)
    /// - `Self::is_valid` must not return false positives, i.e. return `true` for trap representations
    // FIXME: Rename to something more sensible, ReprCTransmute, ExternCTransmute?
    // or integrate it with ExternC?
    pub unsafe trait FlatTransmute: ExternC {
        /// Called when transmuting [`Self::CType`] back into [`Self`] to check for trap representations.
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::CType) -> bool;
    }

    unsafe impl<R: ReprFamily<Kind = Transparent> + CheckedTransmute<Target: FlatTransmute>> FlatTransmute for R {
        fn is_valid(target: &Self::CType) -> bool {
            if !<R::Target as FlatTransmute>::is_valid(target) {
                return false;
            }

            let target_ptr = core::ptr::from_ref(target).cast::<R::Target>();
            <R as CheckedTransmute>::is_valid(unsafe { &*target_ptr })
        }
    }
    unsafe impl<R: ReprFamily<Kind = Robust> + ReprC> FlatTransmute for R {
        fn is_valid(_: &Self::CType) -> bool {
            true
        }
    }
}

unsafe impl<R: CheckedTransmute, const N: usize> CheckedTransmute for [R; N] {
    type Target = [R::Target; N];

    #[inline(always)]
    fn is_valid(target: &Self::Target) -> bool {
        assert_arr_has_non_zero_len::<N>();
        target.iter().all(R::is_valid)
    }
}

#[repr(C)]
union TransmuteHelper<R: CheckedTransmute> {
    source: ManuallyDrop<R>,
    target: ManuallyDrop<R::Target>,
}

pub(super) fn transmute_into_target<R: CheckedTransmute>(source: R) -> R::Target {
    assert_size_and_allignment_match::<R>();

    let transmute_helper = TransmuteHelper {
        source: ManuallyDrop::new(source),
    };

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    ManuallyDrop::into_inner(unsafe { transmute_helper.target })
}
pub(super) fn transmute_from_target<R: CheckedTransmute>(source: R::Target) -> Option<R> {
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

#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
pub(super) fn transmute_into_target_boxed_slice<R: CheckedTransmute>(
    #[expect(clippy::boxed_local)] mut source: Box<[R]>,
) -> Box<[R::Target]> {
    assert_size_and_allignment_match::<R>();

    let (ptr, len) = (source.as_mut_ptr().cast::<R::Target>(), source.len());

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(ptr, len)) }
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
pub(super) fn transmute_from_target_boxed_slice<R: CheckedTransmute>(
    #[expect(clippy::boxed_local)] mut source: Box<[R::Target]>,
) -> Option<Box<[R]>> {
    assert_size_and_allignment_match::<R>();

    if !source.iter().all(|item| R::is_valid(item)) {
        return None;
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Some(unsafe {
        Box::from_raw(core::ptr::slice_from_raw_parts_mut(
            source.as_mut_ptr().cast(),
            source.len(),
        ))
    })
}

pub(super) fn transmute_into_target_ref_slice<R: CheckedTransmute>(source: &[R]) -> &[R::Target] {
    assert_size_and_allignment_match::<R>();

    let (ptr, len) = (source.as_ptr().cast::<R::Target>(), source.len());

    unsafe { core::slice::from_raw_parts(ptr, len) }
}
pub(super) fn transmute_from_target_ref_slice<R: CheckedTransmute>(
    source: &[R::Target],
) -> Option<&[R]> {
    assert_size_and_allignment_match::<R>();

    if !source.iter().all(|item| R::is_valid(item)) {
        return None;
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Some(unsafe { core::slice::from_raw_parts(source.as_ptr().cast(), source.len()) })
}

pub(super) fn transmute_into_target_slice_mut<R: FlatTransmute>(
    source: &mut [R],
) -> &mut [R::CType] {
    let (ptr, len) = (source.as_mut_ptr().cast::<R::CType>(), source.len());

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { core::slice::from_raw_parts_mut(ptr, len) }
}
pub(super) fn transmute_from_target_slice_mut<R: FlatTransmute>(
    source: &mut [R::CType],
) -> Option<&mut [R]> {
    if !source.iter_mut().all(|item| R::is_valid(item)) {
        return None;
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Some(unsafe { core::slice::from_raw_parts_mut(source.as_mut_ptr().cast(), source.len()) })
}

#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
pub(super) fn transmute_into_target_vec<R: CheckedTransmute>(source: Vec<R>) -> Vec<R::Target> {
    assert_size_and_allignment_match::<R>();

    let mut vec = ManuallyDrop::new(source);

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { Vec::from_raw_parts(vec.as_mut_ptr().cast(), vec.len(), vec.capacity()) }
}
#[cfg(feature = "owned_types")]
#[cfg(feature = "owned_as_ref")]
pub(super) fn transmute_from_target_vec<R: CheckedTransmute>(
    source: Vec<R::Target>,
) -> Option<Vec<R>> {
    assert_size_and_allignment_match::<R>();

    if !source.iter().all(|item| R::is_valid(item)) {
        return None;
    }

    let mut vec = ManuallyDrop::new(source);

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Some(unsafe { Vec::from_raw_parts(vec.as_mut_ptr().cast(), vec.len(), vec.capacity()) })
}

fn assert_size_and_allignment_match<R: CheckedTransmute>() {
    const {
        debug_assert!(core::mem::size_of::<R>() == core::mem::size_of::<R::Target>());
        debug_assert!(core::mem::align_of::<R>() == core::mem::align_of::<R::Target>());
    };
}

#[cfg(test)]
mod tests {
    use core::num::NonZeroU8;

    use static_assertions::{assert_impl_all, assert_not_impl_any};

    #[cfg(feature = "non_robust_ref_mut")]
    use crate::slice::CSliceMut;
    use crate::{Decode, Encode, niche::Niche, slice::CSlice};

    use super::*;

    #[derive(ExternC)]
    #[repr(transparent)]
    pub struct TransparentWrapper<T>(T);

    #[test]
    fn transparent_bool() {
        assert_impl_all!(bool: CheckedTransmute<Target = u8>, FlatTransmute<CType = u8>, Niche);
        assert_impl_all!(&bool: CheckedTransmute<Target = &'static u8>, FlatTransmute<CType = *const u8>, StableNiche);
        assert_impl_all!(&mut bool: CheckedTransmute, FlatTransmute<CType = *mut u8>, StableNiche);
        // FIXME:
        //assert_impl_all!(Box<&bool>: CheckedTransmute<Target = Box<*const u8>>, FlatTransmute<CType = *mut *const u8>, StableNiche);
        assert_impl_all!(&[bool]: Niche<CType = CSlice<u8>>);
        #[cfg(feature = "non_robust_ref_mut")]
        assert_impl_all!(&mut [bool]: Niche<CType = CSliceMut<u8>>);
        assert_impl_all!([bool; 2]: CheckedTransmute<Target = [u8; 2]>, FlatTransmute<CType = [u8; 2]>, Niche);
        assert_impl_all!(Option<bool>: Niche<CType = u8>);

        assert_not_impl_any!(bool: ReprC, StableNiche);
        assert_not_impl_any!(&bool: ReprC);
        assert_not_impl_any!(&mut bool: ReprC);
        assert_not_impl_any!(Box<bool>: ReprC);
        assert_not_impl_any!(&[bool]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(&mut [bool]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!([bool; 2]: ReprC, StableNiche);
        assert_not_impl_any!(Option<bool>: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
    }

    #[test]
    fn robust_u8_ref() {
        assert_impl_all!(&u8: FlatTransmute<CType = *const u8>, StableNiche);
        assert_impl_all!(&&u8: CheckedTransmute<Target = &'static *const u8>, FlatTransmute<CType = *const *const u8>, StableNiche);
        assert_impl_all!(&mut &u8: CheckedTransmute, FlatTransmute<CType = *mut *const u8>, StableNiche);
        // FIXME:
        //assert_impl_all!(Box<&u8>: CheckedTransmute<Target = Box<*const u8>>, FlatTransmute<CType = *mut *const u8>, StableNiche);
        assert_impl_all!(&[&u8]: Niche<CType = CSlice<*const u8>>);
        #[cfg(feature = "non_robust_ref_mut")]
        assert_impl_all!(&mut [&u8]: Niche<CType = CSliceMut<*const u8>>);
        assert_impl_all!([&u8; 2]: CheckedTransmute<Target = [*const u8; 2]>, FlatTransmute<CType = [*const u8; 2]>, Niche);
        assert_impl_all!(Option<&u8>: ReprC, CheckedTransmute<Target = *const u8>, FlatTransmute<CType = *const u8>);

        assert_not_impl_any!(&u8: ReprC);
        assert_not_impl_any!(&&u8: ReprC);
        assert_not_impl_any!(&mut &u8: ReprC);
        assert_not_impl_any!(Box<&u8>: ReprC);
        assert_not_impl_any!(&[&u8]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(&mut [&u8]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!([&u8; 2]: ReprC, StableNiche);
        assert_not_impl_any!(Option<&u8>: Niche);
    }

    #[test]
    fn transparent_bool_ref() {
        assert_impl_all!(&bool: CheckedTransmute<Target = &'static u8>, FlatTransmute<CType = *const u8>, StableNiche);
        assert_impl_all!(&&bool: CheckedTransmute<Target = &'static &'static u8>, FlatTransmute<CType = *const *const u8>, StableNiche);
        assert_impl_all!(&mut &bool: CheckedTransmute, FlatTransmute<CType = *mut *const u8>, StableNiche);
        // FIXME:
        //assert_impl_all!(Box<&bool>: CheckedTransmute<Target = Box<*const u8>>, FlatTransmute<CType = *mut *const u8>, StableNiche);
        assert_impl_all!(&[&bool]: Niche<CType = CSlice<*const u8>>);
        #[cfg(feature = "non_robust_ref_mut")]
        assert_impl_all!(&mut [&bool]: Niche<CType = CSliceMut<*const u8>>);
        assert_impl_all!([&bool; 2]: CheckedTransmute<Target = [&'static u8; 2]>, FlatTransmute<CType = [*const u8; 2]>, Niche);
        assert_impl_all!(Option<&bool>: CheckedTransmute<Target = Option<&'static u8>>, FlatTransmute<CType = *const u8>);

        assert_not_impl_any!(&bool: ReprC);
        assert_not_impl_any!(&&bool: ReprC);
        assert_not_impl_any!(&mut &bool: ReprC);
        assert_not_impl_any!(Box<&bool>: ReprC);
        assert_not_impl_any!(&[&bool]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(&mut [&bool]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!([&bool; 2]: ReprC, StableNiche);
        assert_not_impl_any!(Option<&bool>: Niche);
        // FIXME: `Option<&bool>` should NOT implement `ReprC`!!!
        //assert_not_impl_any!(Option<&bool>: ReprC);
    }

    #[test]
    fn mut_ref_behind_ref() {
        //assert_impl_all!(&&mut bool: Decode<'static>);
    }

    #[test]
    #[cfg(not(feature = "non_robust_ref_mut"))]
    fn non_robust_ref_mut() {
        assert_impl_all!(&mut bool: CheckedTransmute<Target = &'static mut u8>, Decode<'static>);
        assert_impl_all!(&mut &bool: CheckedTransmute<Target = &'static mut &'static u8>, Decode<'static>);
        assert_impl_all!(&mut &u8: CheckedTransmute<Target = &'static mut *const u8>, Decode<'static>);
        assert_impl_all!(&mut TransparentWrapper<bool>: CheckedTransmute<Target = &'static mut bool>, Decode<'static>);
        assert_impl_all!(TransparentWrapper<&mut bool>: CheckedTransmute<Target = &'static mut bool>, Decode<'static>, Niche);
        assert_impl_all!(Option<&mut u8>: CheckedTransmute<Target = *mut u8>, FlatTransmute, Decode<'static>);
        assert_impl_all!(Option<&mut bool>: CheckedTransmute<Target = Option<&'static mut u8>>, FlatTransmute, Decode<'static>);
        assert_impl_all!(Option<&mut TransparentWrapper<bool>>: CheckedTransmute<Target = Option<&'static mut bool>>, FlatTransmute, Decode<'static>);
        assert_impl_all!(Option<TransparentWrapper<&mut bool>>: CheckedTransmute<Target = Option<&'static mut bool>>, FlatTransmute, Decode<'static>);
        assert_impl_all!(&mut &mut bool: CheckedTransmute<Target = &'static mut &'static mut u8>, Decode<'static>);
        assert_impl_all!(&mut Option<&mut bool>: CheckedTransmute<Target = &'static mut Option<&'static mut u8>>, Decode<'static>);
        assert_impl_all!(&mut TransparentWrapper<&mut bool>: CheckedTransmute<Target = &'static mut &'static mut bool>, Decode<'static>);

        assert_not_impl_any!(&mut bool: ReprC, Encode);
        assert_not_impl_any!(&mut TransparentWrapper<bool>: ReprC, Encode);
        assert_not_impl_any!(TransparentWrapper<&mut bool>: ReprC, Encode);
        assert_not_impl_any!(&mut &bool: ReprC, Encode);
        assert_not_impl_any!(&mut &u8: ReprC, Encode);
        assert_not_impl_any!(Option<&mut bool>: ReprC, Encode, Niche);
        assert_not_impl_any!(Option<&mut TransparentWrapper<bool>>: ReprC, Encode, Niche);
        assert_not_impl_any!(Option<TransparentWrapper<&mut bool>>: ReprC, Encode, Niche);
        assert_not_impl_any!(&mut &mut bool: ReprC, Encode);
        assert_not_impl_any!(&mut Option<&mut bool>: ReprC, Encode);
        assert_not_impl_any!(&mut TransparentWrapper<&mut bool>: ReprC, Encode);

        assert_not_impl_any!(&mut [bool]: Encode, StableNiche);
        assert_not_impl_any!(&mut [&bool]: Encode, StableNiche);
        assert_not_impl_any!(&mut [&u8]: Encode, StableNiche);
        assert_not_impl_any!(&mut [TransparentWrapper<bool>]: Encode, StableNiche);

        assert_impl_all!(&mut [bool]: Decode<'static>, Niche);
        assert_impl_all!(&mut [&bool]: Decode<'static>, Niche);
        assert_impl_all!(&mut [&u8]: Decode<'static>, Niche);
        assert_impl_all!(&mut [TransparentWrapper<bool>]: Decode<'static>, Niche);
    }

    #[test]
    #[cfg(feature = "non_robust_ref_mut")]
    fn non_robust_ref_mut() {
        assert_impl_all!(&mut bool: CheckedTransmute<Target = &'static mut u8>, Encode, Decode<'static>);
        assert_impl_all!(&mut &bool: CheckedTransmute<Target = &'static mut &'static u8>, Encode, Decode<'static>);
        assert_impl_all!(&mut &u8: CheckedTransmute<Target = &'static mut *const u8>, Encode, Decode<'static>);
        assert_impl_all!(&mut TransparentWrapper<bool>: CheckedTransmute<Target = &'static mut bool>, Encode, Decode<'static>);
        assert_impl_all!(TransparentWrapper<&mut bool>: CheckedTransmute<Target = &'static mut bool>, Encode, Decode<'static>, Niche);
        assert_impl_all!(Option<&mut u8>: CheckedTransmute<Target = *mut u8>, FlatTransmute, Encode, Decode<'static>);
        assert_impl_all!(Option<&mut bool>: CheckedTransmute<Target = Option<&'static mut u8>>, FlatTransmute, Encode, Decode<'static>);
        assert_impl_all!(Option<&mut TransparentWrapper<bool>>: CheckedTransmute<Target = Option<&'static mut bool>>, FlatTransmute, Encode, Decode<'static>);
        assert_impl_all!(Option<TransparentWrapper<&mut bool>>: CheckedTransmute<Target = Option<&'static mut bool>>, FlatTransmute, Encode, Decode<'static>);
        assert_impl_all!(&mut &mut bool: CheckedTransmute<Target = &'static mut &'static mut u8>, Decode<'static>);
        assert_impl_all!(&mut Option<&mut bool>: CheckedTransmute<Target = &'static mut Option<&'static mut u8>>, Decode<'static>);
        assert_impl_all!(&mut TransparentWrapper<&mut bool>: CheckedTransmute<Target = &'static mut &'static mut bool>, Decode<'static>);

        assert_not_impl_any!(&mut [bool]: StableNiche);
        assert_not_impl_any!(&mut [&bool]: StableNiche);
        assert_not_impl_any!(&mut [&u8]: StableNiche);
        assert_not_impl_any!(&mut [TransparentWrapper<bool>]: StableNiche);

        assert_impl_all!(&mut [bool]: Encode, Decode<'static>, Niche);
        assert_impl_all!(&mut [&bool]: Encode, Decode<'static>, Niche);
        assert_impl_all!(&mut [&u8]: Encode, Decode<'static>, Niche);
        assert_impl_all!(&mut [TransparentWrapper<bool>]: Encode, Decode<'static>, Niche);
    }

    #[test]
    fn unsupported_ref_mut() {
        assert_not_impl_any!(&mut (u8,): ReprC, ExternC);
        assert_not_impl_any!(&mut (NonZeroU8,): ReprC, ExternC);
        assert_not_impl_any!(&mut Option<u8>: ReprC, ExternC);
        assert_not_impl_any!(&mut Option<bool>: ReprC, ExternC);
    }
}
