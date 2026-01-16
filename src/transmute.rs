use core::mem::ManuallyDrop;

use alloc::boxed::Box;
#[cfg(feature = "owned_as_ref")]
use alloc::vec::Vec;
use disjoint_impls::disjoint_impls;

use crate::{
    ExternC, ReprC, assert_arr_has_non_zero_len,
    ir::{Opaque, ReprFamily, Robust, Transmuted},
    niche::StableNiche,
};

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
        ///
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::Target) -> bool;
    }

    unsafe impl<'a, R: ReprFamily<Kind = Transmuted> + CheckedTransmute> CheckedTransmute for &'a R {
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

    unsafe impl<'a, R: ReprFamily<Kind = Transmuted> + CheckedTransmute> CheckedTransmute for &'a mut R {
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

    unsafe impl<R: ReprFamily<Kind = Transmuted> + CheckedTransmute> CheckedTransmute for Box<R> {
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

    unsafe impl<R: CheckedTransmute<Target: ReprFamily<Kind = Transmuted>> + StableNiche> CheckedTransmute for Option<R> {
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
        ///
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::CType) -> bool;
    }

    unsafe impl<R: ReprFamily<Kind = Transmuted> + CheckedTransmute<Target: FlatTransmute>> FlatTransmute for R {
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

disjoint_impls! {
    /// Marker trait for types that are safe to mutatate
    ///
    /// Trait is used to prevent handing out (i.e. encoding) mutable references to non-robust types
    /// across FFI boundary. This prevents UB that might arise from caller setting the referent to
    /// a trap representation.
    ///
    /// Types safe to mutate include:
    /// * `&T` (it's guaranteed to be immutable)
    /// * `&mut T` where `T: ReprC` (robust referent)
    /// * any type that contains only `MutSafe` types
    ///
    /// When `non_robust_ref_mut` feature is active this trait is unused, i.e. an implementation
    /// of [`crate::Encode`] is provided even for mutable references to non-robust types. Use it
    /// at your own discretion
    ///
    /// # Safety
    ///
    /// The type must not carry a mutable reference to a non-robust type
    pub unsafe trait MutSafe {}

    unsafe impl<R: ReprFamily<Kind = Robust>> MutSafe for R {}

    unsafe impl<R> MutSafe for &R where Self: ReprFamily<Kind = Transmuted> {}
    unsafe impl<R: ReprC> MutSafe for &mut R where Self: ReprFamily<Kind = Transmuted> {}
    // WARN: Since `Box<&mut R>` is mapped to `*mut *mut R` this can be disputed in the case of
    // no ownership transfer where Box's invariant can be violated by the caller by NULLing the
    // inner pointer. However, because the box is immediately dropped following the function call,
    // we deem it ok as it would most likely lead to a catastrophic segfault, not a silent UB.
    unsafe impl<R: crate::Encode> MutSafe for Box<R> where Self: ReprFamily<Kind = Transmuted> {}
    unsafe impl<R: crate::Encode> MutSafe for Option<R> where Self: ReprFamily<Kind = Transmuted> {}
    unsafe impl<R: crate::Encode, const N: usize> MutSafe for [R; N] where Self: ReprFamily<Kind = Transmuted> {}
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

#[cfg(feature = "owned_as_ref")]
pub(super) fn transmute_into_target_boxed_slice<R: CheckedTransmute>(
    #[expect(clippy::boxed_local)] mut source: Box<[R]>,
) -> Box<[R::Target]> {
    assert_size_and_allignment_match::<R>();

    let (ptr, len) = (source.as_mut_ptr().cast::<R::Target>(), source.len());

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(ptr, len)) }
}
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

#[cfg(feature = "owned_as_ref")]
pub(super) fn transmute_into_target_vec<R: CheckedTransmute>(source: Vec<R>) -> Vec<R::Target> {
    assert_size_and_allignment_match::<R>();

    let mut vec = ManuallyDrop::new(source);

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { Vec::from_raw_parts(vec.as_mut_ptr().cast(), vec.len(), vec.capacity()) }
}
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

    #[cfg(feature = "derive")]
    #[derive(ExternC)]
    #[repr(transparent)]
    pub struct TransparentWrapper<T>(T);

    #[cfg(feature = "derive")]
    #[derive(PartialEq, ExternC)]
    #[repr(C)]
    pub struct MyStruct<T>(T, u32);

    #[test]
    fn transparent_bool() {
        assert_impl_all!(bool: CheckedTransmute<Target = u8>, FlatTransmute<CType = u8>, Niche, Decode<'static>);
        assert_impl_all!(&bool: CheckedTransmute<Target = &'static u8>, FlatTransmute<CType = *const u8>, StableNiche, Decode<'static>);
        assert_impl_all!(&mut bool: CheckedTransmute<Target = &'static mut u8>, Decode<'static>, FlatTransmute<CType = *mut u8>, StableNiche, Decode<'static>);
        // FIXME:
        //assert_impl_all!(Box<&bool>: CheckedTransmute<Target = Box<*const u8>>, FlatTransmute<CType = *mut *const u8>, StableNiche, Decode<'static>);
        assert_impl_all!(&[bool]: Niche<CType = CSlice<u8>>, Decode<'static>);
        #[cfg(feature = "non_robust_ref_mut")]
        assert_impl_all!(&mut [bool]: Niche<CType = CSliceMut<u8>>, Decode<'static>);
        assert_impl_all!([bool; 2]: CheckedTransmute<Target = [u8; 2]>, FlatTransmute<CType = [u8; 2]>, Niche, Decode<'static>);
        assert_impl_all!(Option<bool>: Niche<CType = u8>, Decode<'static>);

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
        assert_impl_all!(&u8: CheckedTransmute<Target = *const u8>, FlatTransmute<CType = *const u8>, StableNiche, Decode<'static>);
        assert_impl_all!(&&u8: CheckedTransmute<Target = &'static *const u8>, FlatTransmute<CType = *const *const u8>, StableNiche, Decode<'static>);
        assert_impl_all!(&mut &u8: CheckedTransmute<Target = &'static mut *const u8>, FlatTransmute<CType = *mut *const u8>, StableNiche, Decode<'static>);
        // FIXME:
        //assert_impl_all!(Box<&u8>: CheckedTransmute<Target = Box<*const u8>>, FlatTransmute<CType = *mut *const u8>, StableNiche, Decode<'static>);
        assert_impl_all!(&[&u8]: Niche<CType = CSlice<*const u8>>, Decode<'static>);
        #[cfg(feature = "non_robust_ref_mut")]
        assert_impl_all!(&mut [&u8]: Niche<CType = CSliceMut<*const u8>>, Decode<'static>);
        assert_impl_all!([&u8; 2]: CheckedTransmute<Target = [*const u8; 2]>, FlatTransmute<CType = [*const u8; 2]>, Niche, Decode<'static>);
        assert_impl_all!(Option<&u8>: ReprC, CheckedTransmute<Target = *const u8>, FlatTransmute<CType = *const u8>, Decode<'static>);

        assert_not_impl_any!(&u8: ReprC);
        assert_not_impl_any!(&&u8: ReprC);
        assert_not_impl_any!(&mut &u8: ReprC);
        assert_not_impl_any!(Box<&u8>: ReprC);
        assert_not_impl_any!(&[&u8]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!(&mut [&u8]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
        assert_not_impl_any!([&u8; 2]: ReprC, StableNiche);
        assert_not_impl_any!(Option<&u8>: Niche);

        assert_impl_all!(&&mut u8: CheckedTransmute<Target = &'static *mut u8 >, FlatTransmute<CType = *const *mut u8>, StableNiche, Encode, Decode<'static>);
        // TODO: Should this type be MutSafe? It's not because Option<&mut u8> is not ReprC because `&mut T` isn't copy
        assert_impl_all!(Option<&mut u8>: CheckedTransmute<Target = *mut u8>, FlatTransmute<CType = *mut u8>, Decode<'static>);
    }

    #[test]
    fn transparent_bool_ref() {
        assert_impl_all!(&bool: CheckedTransmute<Target = &'static u8>, FlatTransmute<CType = *const u8>, StableNiche, Decode<'static>);
        assert_impl_all!(&&bool: CheckedTransmute<Target = &'static &'static u8>, FlatTransmute<CType = *const *const u8>, StableNiche, Decode<'static>);
        assert_impl_all!(&mut &bool: CheckedTransmute<Target = &'static mut &'static u8>, FlatTransmute<CType = *mut *const u8>, StableNiche, Decode<'static>);
        // FIXME:
        //assert_impl_all!(Box<&bool>: CheckedTransmute<Target = Box<*const u8>>, FlatTransmute<CType = *mut *const u8>, StableNiche, Decode<'static>);
        assert_impl_all!(&[&bool]: Niche<CType = CSlice<*const u8>>, Decode<'static>);
        #[cfg(feature = "non_robust_ref_mut")]
        assert_impl_all!(&mut [&bool]: Niche<CType = CSliceMut<*const u8>>, Decode<'static>);
        assert_impl_all!([&bool; 2]: CheckedTransmute<Target = [&'static u8; 2]>, FlatTransmute<CType = [*const u8; 2]>, Niche, Decode<'static>);
        assert_impl_all!(Option<&bool>: CheckedTransmute<Target = Option<&'static u8>>, FlatTransmute<CType = *const u8>, Decode<'static>);

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

        assert_impl_all!(Option<&mut bool>: CheckedTransmute<Target = Option<&'static mut u8>>, FlatTransmute<CType = *mut u8>, Decode<'static>);
        assert_impl_all!(&mut Option<&mut bool>: CheckedTransmute<Target = &'static mut Option<&'static mut u8>>, FlatTransmute<CType = *mut *mut u8>, StableNiche, Decode<'static>);
        assert_impl_all!(&&mut bool: CheckedTransmute<Target = &'static &'static mut u8 >, FlatTransmute<CType = *const *mut u8>, StableNiche, Encode, Decode<'static>);
        assert_impl_all!(&mut &mut bool: CheckedTransmute<Target = &'static mut &'static mut u8>, FlatTransmute<CType = *mut *mut u8>, StableNiche, Decode<'static>);

        assert_not_impl_any!(Option<&mut bool>: ReprC, Niche);
        assert_not_impl_any!(&mut &mut bool: ReprC);
        assert_not_impl_any!(&mut Option<&mut bool>: ReprC);
    }

    #[test]
    #[cfg(feature = "derive")]
    fn transparent_wrapper() {
        assert_impl_all!(&mut TransparentWrapper<bool>: CheckedTransmute<Target = &'static mut bool>, FlatTransmute<CType = *mut u8>, StableNiche, Decode<'static>);
        assert_impl_all!(TransparentWrapper<&mut bool>: CheckedTransmute<Target = &'static mut bool>, FlatTransmute<CType = *mut u8>, StableNiche, Decode<'static>);
        assert_impl_all!(&mut TransparentWrapper<&mut bool>: CheckedTransmute<Target = &'static mut &'static mut bool>, FlatTransmute<CType = *mut *mut u8>, StableNiche, Decode<'static>);
        assert_impl_all!(Option<&mut TransparentWrapper<bool>>: CheckedTransmute<Target = Option<&'static mut bool>>, FlatTransmute<CType = *mut u8>, Decode<'static>);
        assert_impl_all!(Option<TransparentWrapper<&mut bool>>: CheckedTransmute<Target = Option<&'static mut bool>>, FlatTransmute<CType = *mut u8>, Decode<'static>);

        assert_not_impl_any!(&mut TransparentWrapper<bool>: ReprC);
        assert_not_impl_any!(TransparentWrapper<&mut bool>: ReprC);
        assert_not_impl_any!(Option<&mut TransparentWrapper<bool>>: ReprC, Niche);
        assert_not_impl_any!(Option<TransparentWrapper<&mut bool>>: ReprC, Niche);
        assert_not_impl_any!(&mut TransparentWrapper<&mut bool>: ReprC);

        assert_impl_all!(&mut [TransparentWrapper<bool>]: Niche, Decode<'static>);
        assert_not_impl_any!(&mut [TransparentWrapper<bool>]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
    }

    #[test]
    #[cfg(feature = "derive")]
    fn repr_c_struct() {
        assert_impl_all!(&mut MyStruct<bool>: CheckedTransmute<Target = &'static mut CMyStruct<bool>>, FlatTransmute<CType = *mut CMyStruct<bool>>, StableNiche, Decode<'static>);
        assert_impl_all!(MyStruct<&mut bool>: CheckedTransmute<Target = CMyStruct<&'static mut bool>>, FlatTransmute<CType = CMyStruct<&'static mut bool>>, Niche, Decode<'static>);
        assert_impl_all!(&mut MyStruct<&mut bool>: CheckedTransmute<Target = &'static mut CMyStruct<&'static mut bool>>, FlatTransmute<CType = *mut CMyStruct<&'static mut bool>>, StableNiche, Decode<'static>);
        assert_impl_all!(Option<&mut MyStruct<bool>>: CheckedTransmute<Target = Option<&'static mut CMyStruct<bool>>>, FlatTransmute<CType = *mut CMyStruct<bool>>, Decode<'static>);
        // FIXME:
        //assert_impl_all!(Option<MyStruct<&mut bool>>: ExternC<CType = CMyStruct<&'static mut bool>>, Decode<'static>);

        assert_not_impl_any!(&mut MyStruct<bool>: ReprC);
        assert_not_impl_any!(MyStruct<&mut bool>: ReprC, StableNiche);
        assert_not_impl_any!(&mut MyStruct<&mut bool>: ReprC);
        assert_not_impl_any!(Option<&mut MyStruct<bool>>: ReprC, Niche);
        assert_not_impl_any!(Option<MyStruct<&mut bool>>: ReprC, CheckedTransmute, FlatTransmute, Niche);

        assert_impl_all!(&mut [MyStruct<bool>]: Niche, Decode<'static>);
        assert_not_impl_any!(&mut [MyStruct<bool>]: ReprC, CheckedTransmute, FlatTransmute, StableNiche);
    }

    #[test]
    #[cfg(not(feature = "non_robust_ref_mut"))]
    fn non_robust_ref_mut() {
        assert_not_impl_any!(&mut bool: Encode);
        assert_not_impl_any!(&mut &bool: Encode);
        assert_not_impl_any!(&mut &u8: Encode);

        assert_not_impl_any!(Option<&mut bool>: Encode);

        assert_not_impl_any!(&mut &mut bool: Encode);
        assert_not_impl_any!(&mut Option<&mut bool>: Encode);

        assert_not_impl_any!(&mut [bool]: Encode);
        assert_not_impl_any!(&mut [&bool]: Encode);
        assert_not_impl_any!(&mut [&u8]: Encode);
    }

    #[test]
    #[cfg(feature = "non_robust_ref_mut")]
    fn non_robust_ref_mut() {
        assert_impl_all!(&mut bool: Encode);
        assert_impl_all!(&mut &bool: Encode);
        assert_impl_all!(&mut &u8: Encode);

        assert_impl_all!(Option<&mut u8>: Encode);
        assert_impl_all!(Option<&mut bool>: Encode);
        assert_impl_all!(&mut &mut bool: Encode);
        assert_impl_all!(&mut Option<&mut bool>: Encode);

        assert_impl_all!(&mut [bool]: Encode);
        assert_impl_all!(&mut [&bool]: Encode);
        assert_impl_all!(&mut [&u8]: Encode);
    }

    #[test]
    #[cfg(feature = "derive")]
    #[cfg(not(feature = "non_robust_ref_mut"))]
    fn non_robust_ref_mut_derive() {
        assert_not_impl_any!(&mut TransparentWrapper<bool>: Encode);
        assert_not_impl_any!(TransparentWrapper<&mut bool>: Encode);
        assert_not_impl_any!(Option<&mut TransparentWrapper<bool>>: Encode);
        assert_not_impl_any!(Option<TransparentWrapper<&mut bool>>: Encode);
        assert_not_impl_any!(&mut TransparentWrapper<&mut bool>: Encode);
        assert_not_impl_any!(&mut [TransparentWrapper<bool>]: Encode);

        assert_not_impl_any!(&mut MyStruct<bool>: Encode);
        assert_not_impl_any!(MyStruct<&mut bool>: Encode);
        assert_not_impl_any!(&mut MyStruct<&mut bool>: Encode);
        assert_not_impl_any!(Option<&mut MyStruct<bool>>: Encode);
        assert_not_impl_any!(Option<MyStruct<&mut bool>>: Encode);
        assert_not_impl_any!(&mut [MyStruct<bool>]: Encode);
    }

    #[test]
    #[cfg(feature = "derive")]
    #[cfg(feature = "non_robust_ref_mut")]
    fn non_robust_ref_mut_derive() {
        assert_impl_all!(&mut TransparentWrapper<bool>: Encode);
        assert_impl_all!(TransparentWrapper<&mut bool>: Encode);
        assert_impl_all!(Option<&mut TransparentWrapper<bool>>: Encode);
        assert_impl_all!(Option<TransparentWrapper<&mut bool>>: Encode);
        assert_impl_all!(&mut TransparentWrapper<&mut bool>: Encode);

        assert_impl_all!(&mut MyStruct<bool>: Encode);
        assert_impl_all!(MyStruct<&mut bool>: Encode);
        assert_impl_all!(&mut MyStruct<&mut bool>: Encode);
        assert_impl_all!(Option<&mut MyStruct<bool>>: Encode);
        assert_impl_all!(Option<MyStruct<&mut bool>>: Encode);
        assert_impl_all!(&mut [MyStruct<bool>]: Encode);
        assert_impl_all!(&mut [TransparentWrapper<bool>]: Encode);
    }

    #[test]
    fn unsupported_ref_mut() {
        assert_not_impl_any!(&mut (u8,): ReprC, ExternC);
        assert_not_impl_any!(&mut (NonZeroU8,): ReprC, ExternC);
        assert_not_impl_any!(&mut Option<u8>: ReprC, ExternC);
        assert_not_impl_any!(&mut Option<bool>: ReprC, ExternC);
    }
}
