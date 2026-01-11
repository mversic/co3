use core::mem::ManuallyDrop;

use disjoint_impls::disjoint_impls;

use super::*;
use crate::{ReprC, niche::StableNiche};

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

    unsafe impl<
        'a,
        #[cfg(not(feature = "non_robust_ref_mut"))] R: ReprC,
        #[cfg(feature = "non_robust_ref_mut")] R,
    > CheckedTransmute for &'a mut R
    where
        R: ReprFamily<Kind = Transparent> + CheckedTransmute,
    {
        type Target = &'a mut R::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }
    #[cfg(feature = "non_robust_ref_mut")]
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
pub(super) fn transmute_from_target<R: CheckedTransmute>(source: R::Target) -> Result<R> {
    assert_size_and_allignment_match::<R>();

    if !R::is_valid(&source) {
        return Err(FfiReturn::TrapRepresentation);
    }

    let transmute_helper = TransmuteHelper {
        target: ManuallyDrop::new(source),
    };

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(ManuallyDrop::into_inner(unsafe { transmute_helper.source }))
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
) -> Result<Box<[R]>> {
    assert_size_and_allignment_match::<R>();

    if !source.iter().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(unsafe {
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
) -> Result<&[R]> {
    assert_size_and_allignment_match::<R>();

    if !source.iter().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(unsafe { core::slice::from_raw_parts(source.as_ptr().cast(), source.len()) })
}

pub(super) fn transmute_into_target_slice_mut<R: CheckedTransmute>(
    source: &mut [R],
) -> &mut [R::Target] {
    assert_size_and_allignment_match::<R>();

    let (ptr, len) = (source.as_mut_ptr().cast::<R::Target>(), source.len());

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { core::slice::from_raw_parts_mut(ptr, len) }
}
pub(super) fn transmute_from_target_slice_mut<R: CheckedTransmute>(
    source: &mut [R::Target],
) -> Result<&mut [R]> {
    assert_size_and_allignment_match::<R>();

    if !source.iter_mut().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(unsafe { core::slice::from_raw_parts_mut(source.as_mut_ptr().cast(), source.len()) })
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
) -> Result<Vec<R>> {
    assert_size_and_allignment_match::<R>();

    if !source.iter().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    let mut vec = ManuallyDrop::new(source);

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(unsafe { Vec::from_raw_parts(vec.as_mut_ptr().cast(), vec.len(), vec.capacity()) })
}

fn assert_size_and_allignment_match<R: CheckedTransmute>() {
    const {
        debug_assert!(core::mem::size_of::<R>() == core::mem::size_of::<R::Target>());
        debug_assert!(core::mem::align_of::<R>() == core::mem::align_of::<R::Target>());
    };
}
