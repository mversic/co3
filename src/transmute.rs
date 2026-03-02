#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use core::mem::ManuallyDrop;

use disjoint_impls::disjoint_impls;

use crate::{
    Encode, ReprC, Store, assert_arr_has_non_zero_len,
    ir::{Cloned, NonRobust, Opaque, ReprFamily, Robust, Transmuted},
    niche::{NicheFamily, StableNiche, WithNiche, WithoutNiche},
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
        type Target;

        /// Called when transmuting [`Self::Target`] back into [`Self`] to check for trap representations.
        ///
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::Target) -> bool;
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
    unsafe impl<'a, R: ReprFamily<Kind = Transmuted> + CheckedTransmute> CheckedTransmute for &'a R {
        type Target = &'a R::Target;

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
    unsafe impl<'a, R: ReprFamily<Kind = Transmuted> + CheckedTransmute> CheckedTransmute for &'a mut R {
        type Target = &'a mut R::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            R::is_valid(target)
        }
    }

    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprFamily<Kind = Robust> + ReprC> CheckedTransmute for Box<R> {
        type Target = *mut R;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprFamily<Kind = Opaque>> CheckedTransmute for Box<R> {
        type Target = *mut R;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
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

    unsafe impl<R: CheckedTransmute<Target: ReprFamily<Kind = Robust> + ReprC> + StableNiche> CheckedTransmute for Option<R> {
        type Target = R::Target;

        #[inline(always)]
        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }
    unsafe impl<R: CheckedTransmute<Target: ReprFamily<Kind = Transmuted>> + StableNiche> CheckedTransmute for Option<R> {
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
        type Target;

        /// Called when transmuting [`Self::Target`] back into [`Self`] to check for trap representations.
        ///
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::Target) -> bool;
    }

    unsafe impl<R: ReprFamily<Kind = Robust>> FlatTransmute for R {
        type Target = Self;

        #[inline(always)]
        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }
    unsafe impl<R: ReprFamily<Kind = Opaque>> FlatTransmute for R {
        type Target = Self;

        #[inline(always)]
        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }
    unsafe impl<R: ReprFamily<Kind: Cloned>> FlatTransmute for R {
        type Target = Self;

        #[inline(always)]
        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }

    unsafe impl<R: ReprFamily<Kind = Transmuted> + CheckedTransmute<Target: FlatTransmute>> FlatTransmute for R {
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
}

pub struct TransmutedRefMutStore<'a, R> {
    _target: Option<R>,
    _original: Option<&'a mut R>,
}

impl<'slice, R> Default for TransmutedRefMutStore<'slice, R> {
    fn default() -> Self {
        Self {
            _target: None,
            _original: None,
        }
    }
}

impl<'a, R> Store for TransmutedRefMutStore<'a, R> {
    fn sync(self) -> Option<()> {
        unimplemented!()
    }
}

disjoint_impls! {
    /// Marker trait for transmuted types that can be encoded safely
    ///
    /// Trait is used to prevent handing out (i.e. encoding) mutable references to non-robust types
    /// across FFI boundary. This prevents UB that might arise from caller setting the referent to
    /// a trap representation.
    ///
    /// Types safe to mutate include:
    /// * `&T` (it's guaranteed to be immutable)
    /// * `&mut T` where `T: ReprC` (robust referent)
    /// * any type that contains only `EncodeTransmuted` types
    ///
    /// When `unsafe-optimizations` feature is active this trait is unused, i.e. an implementation
    /// of [`crate::Encode`] is provided even for mutable references to non-robust types. Use it
    /// at your own discretion
    ///
    /// # Safety
    ///
    /// The type must not carry a mutable reference to a non-robust type
    pub unsafe trait EncodeTransmuted: CheckedTransmute + Sized {
        // TODO: Use default associated type when available
        // https://github.com/rust-lang/rust/issues/29661
        type Store: Store + Default;

        fn encode_transmuted<'itm>(self, _store: &'itm mut Self::Store) -> Self::Target
        where
            Self: 'itm,
        {
            transmute_into_target(self)
        }
    }

    unsafe impl<R: ReprFamily<Kind: NonRobust>> EncodeTransmuted for &mut R where Self: CheckedTransmute<Target: Encode> {
        type Store = <Self::Target as crate::Encode>::Store;
    }
    unsafe impl<R: ReprFamily<Kind = Robust> + NicheFamily<Kind = WithoutNiche> + ReprC> EncodeTransmuted for &mut R where Self: CheckedTransmute<Target = *mut R> {
        type Store = <Self::Target as crate::Encode>::Store;
    }
    unsafe impl<'a, R: ReprFamily<Kind = Robust> + NicheFamily<Kind: WithNiche> + ReprC> EncodeTransmuted for &'a mut R where Self: CheckedTransmute<Target = *mut R> {
        #[cfg(not(feature = "unsafe-optimizations"))]
        type Store = TransmutedRefMutStore<'a, R>;
        #[cfg(feature = "unsafe-optimizations")]
        type Store = ();

        fn encode_transmuted<'itm>(self, _store: &'itm mut Self::Store) -> Self::Target
        where
            Self: 'itm,
        {
            #[cfg(not(feature = "unsafe-optimizations"))]
            let ctype: &mut R = {
                let original: &mut R = _store._original.insert(self);
                _store._target.insert(*original)
            };
            #[cfg(feature = "unsafe-optimizations")]
            let ctype = self;

            ctype
        }
    }
}

unsafe impl<R> EncodeTransmuted for &R
where
    Self: CheckedTransmute<Target: Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;
}
// WARN: Since `Box<&mut R>` is mapped to `*mut *mut R` this can be disputed in the case of
// no ownership transfer where Box's invariant can be violated by the caller by NULLing the
// inner pointer. However, because the box is immediately dropped following the function call,
// we deem it ok as it would most likely lead to a catastrophic segfault, not a silent UB.
#[cfg(feature = "alloc")]
unsafe impl<R: EncodeTransmuted> EncodeTransmuted for Box<R>
where
    Self: CheckedTransmute<Target: Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;

    fn encode_transmuted<'itm>(self, _store: &'itm mut Self::Store) -> Self::Target
    where
        Self: 'itm,
    {
        unimplemented!()
    }
}
unsafe impl<R> EncodeTransmuted for Option<R>
where
    Self: CheckedTransmute<Target: Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;
}
unsafe impl<R, const N: usize> EncodeTransmuted for [R; N]
where
    Self: CheckedTransmute<Target: Encode>,
{
    type Store = <Self::Target as crate::Encode>::Store;
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

pub(crate) fn transmute_into_target<R: CheckedTransmute>(source: R) -> R::Target {
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

#[cfg(feature = "alloc")]
pub(super) fn transmute_into_target_boxed_slice<R: CheckedTransmute>(
    #[expect(clippy::boxed_local)] mut source: Box<[R]>,
) -> Box<[R::Target]> {
    assert_size_and_allignment_match::<R>();

    let (ptr, len) = (source.as_mut_ptr().cast(), source.len());

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { Box::from_raw(core::ptr::slice_from_raw_parts_mut(ptr, len)) }
}
#[cfg(feature = "alloc")]
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

    let (ptr, len) = (source.as_ptr().cast(), source.len());

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

pub(super) fn transmute_into_target_slice_mut<R: CheckedTransmute>(
    source: &mut [R],
) -> &mut [R::Target] {
    let (ptr, len) = (source.as_mut_ptr().cast(), source.len());

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { core::slice::from_raw_parts_mut(ptr, len) }
}
pub(super) fn transmute_from_target_slice_mut<R: CheckedTransmute>(
    source: &mut [R::Target],
) -> Option<&mut [R]> {
    if !source.iter_mut().all(|item| R::is_valid(item)) {
        return None;
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Some(unsafe { core::slice::from_raw_parts_mut(source.as_mut_ptr().cast(), source.len()) })
}

#[cfg(feature = "alloc")]
pub(super) fn transmute_into_target_vec<R: CheckedTransmute>(source: Vec<R>) -> Vec<R::Target> {
    assert_size_and_allignment_match::<R>();

    let mut vec = ManuallyDrop::new(source);

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { Vec::from_raw_parts(vec.as_mut_ptr().cast(), vec.len(), vec.capacity()) }
}
#[cfg(feature = "alloc")]
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
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::CBoxedSlice;
    use crate::{
        Decode, Encode, ExternC,
        ir::{ReprFamily, Transmuted},
        niche::{Niche, NicheFamily, WithCustomNiche, WithStableNiche, WithoutNiche},
        slice::{CSlice, CSliceMut},
        vec::CVec,
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
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<bool>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut u8>,
        //    Decode<'static>,
        //    Encode,
        //);
        assert_impl_all!(&[bool]:
            ReprFamily<Kind = &'static [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            // FIXME:
            //Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [bool]:
            ReprFamily<Kind = &'static mut [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[bool]>:
            ReprFamily<Kind = Box<[Transmuted]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<bool>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CVec<u8>>,
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
            ReprFamily<Kind = Option<WithCustomNiche>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = u8>,
            Decode<'static>,
            Encode,
        );

        assert_not_impl_any!(bool: ReprC, StableNiche);
        assert_not_impl_any!(&bool: ReprC);
        assert_not_impl_any!(&mut bool: ReprC);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<bool>: ReprC);
        assert_not_impl_any!(&[bool]: ReprC, StableNiche);
        assert_not_impl_any!(&mut [bool]: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[bool]>: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<bool>: ReprC, StableNiche);
        assert_not_impl_any!([bool; 2]: ReprC, StableNiche);
        assert_not_impl_any!(Option<bool>: ReprC, StableNiche);

        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(&mut bool: Encode);
        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(&mut [bool]: Encode);
        // FIXME:
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut bool: Encode);
        #[cfg(not(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        )))]
        assert_not_impl_any!(&mut [bool]: Encode);
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
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<&bool>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut *const u8>,
        //    Decode<'static>,
        //    Encode,
        //);
        assert_impl_all!(&[&u8]:
            ReprFamily<Kind = &'static [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&u8]:
            ReprFamily<Kind = &'static mut [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&u8]>:
            ReprFamily<Kind = Box<[Transmuted]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&u8>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CVec<*const u8>>,
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

        assert_not_impl_any!(&&u8: ReprC);
        assert_not_impl_any!(&mut &u8: ReprC);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<&u8>: ReprC);
        assert_not_impl_any!(&[&u8]: ReprC, StableNiche);
        assert_not_impl_any!(&mut [&u8]: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[&u8]>: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<&u8>: ReprC, StableNiche);
        assert_not_impl_any!([&u8; 2]: ReprC, StableNiche);
        assert_not_impl_any!(Option<&u8>: ReprC, Niche);

        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(&mut &u8: Encode);
        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(&mut [&u8]: Encode);
        // FIXME:
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut &u8: Encode);
        #[cfg(not(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        )))]
        assert_not_impl_any!(&mut [&u8]: Encode);
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
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<&bool>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut *const u8>,
        //    Decode<'static>,
        //    Encode,
        //);
        assert_impl_all!(&[&bool]:
            ReprFamily<Kind = &'static [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&bool]:
            ReprFamily<Kind = &'static mut [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&bool]>:
            ReprFamily<Kind = Box<[Transmuted]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&bool>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CVec<*const u8>>,
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

        assert_not_impl_any!(&&bool: ReprC);
        assert_not_impl_any!(&mut &bool: ReprC);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<&bool>: ReprC);
        assert_not_impl_any!(&[&bool]: ReprC, StableNiche);
        assert_not_impl_any!(&mut [&bool]: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[&bool]>: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<&bool>: ReprC, StableNiche);
        assert_not_impl_any!([&bool; 2]: ReprC, StableNiche);
        assert_not_impl_any!(Option<&bool>: ReprC, Niche);

        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(&mut &bool: Encode);
        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(&mut [&bool]: Encode);
        // FIXME:
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut &bool: Encode);
        #[cfg(not(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        )))]
        assert_not_impl_any!(&mut [&bool]: Encode);
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
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<&mut u8>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut *mut u8>,
        //    Decode<'static>,
        //    Encode,
        //);
        assert_impl_all!(&[&mut u8]:
            ReprFamily<Kind = &'static [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut u8]:
            ReprFamily<Kind = &'static mut [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*mut u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut u8]>:
            ReprFamily<Kind = Box<[Transmuted]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut u8>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CVec<*mut u8>>,
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

        assert_not_impl_any!(&&mut u8: ReprC);
        assert_not_impl_any!(&mut &mut u8: ReprC);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<&mut u8>: ReprC);
        assert_not_impl_any!(&[&mut u8]: ReprC, StableNiche);
        assert_not_impl_any!(&mut [&mut u8]: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[&mut u8]>: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<&mut u8>: ReprC, StableNiche);
        assert_not_impl_any!([&mut u8; 2]: ReprC, StableNiche);
        assert_not_impl_any!(Option<&mut u8>: ReprC, Niche);

        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(&mut &mut u8: Encode);
        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(&mut [&mut u8]: Encode);
        // FIXME:
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut &mut u8: Encode);
        #[cfg(not(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        )))]
        assert_not_impl_any!(&mut [&mut u8]: Encode);
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
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<&mut bool>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut *mut u8>,
        //    Decode<'static>,
        //);
        assert_impl_all!(&[&mut bool]:
            ReprFamily<Kind = &'static [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*mut u8>>,
            Decode<'static>,
            Encode,
        );
        assert_impl_all!(&mut [&mut bool]:
            ReprFamily<Kind = &'static mut [Transmuted]>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*mut u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut bool]>:
            ReprFamily<Kind = Box<[Transmuted]>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            Decode<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut bool>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CVec<*mut u8>>,
            Decode<'static>,
        );
        assert_impl_all!([&mut bool; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*mut u8; 2]>,
            Decode<'static>,
        );
        assert_impl_all!(Option<&mut bool>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *mut u8>,
            Decode<'static>,
        );

        assert_not_impl_any!(&&mut bool: ReprC);
        assert_not_impl_any!(&mut &mut bool: ReprC);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<&mut bool>: ReprC);
        assert_not_impl_any!(&[&mut bool]: ReprC, StableNiche);
        assert_not_impl_any!(&mut [&mut bool]: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Box<[&mut bool]>: ReprC, StableNiche);
        #[cfg(feature = "alloc")]
        assert_not_impl_any!(Vec<&mut bool>: ReprC, StableNiche);
        assert_not_impl_any!([&mut bool; 2]: ReprC, StableNiche);
        assert_not_impl_any!(Option<&mut bool>: ReprC, Niche);

        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(&mut &mut bool: Encode);
        // FIXME:
        //#[cfg(feature = "alloc")]
        //#[cfg(any(feature = "unsafe-optimizations", feature = "unstable-refs"))]
        //assert_impl_all!(Box<&mut bool>: Encode);
        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(&mut [&mut bool]: Encode);
        #[cfg(feature = "alloc")]
        #[cfg(any(feature = "unsafe-optimizations", feature = "unstable-refs"))]
        assert_impl_all!(Box<[&mut bool]>: Encode);
        #[cfg(feature = "alloc")]
        #[cfg(any(feature = "unsafe-optimizations", feature = "unstable-refs"))]
        assert_impl_all!(Vec<&mut bool>: Encode);
        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!([&mut bool; 2]: Encode);
        #[cfg(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        ))]
        assert_impl_all!(Option<&mut bool>: Encode);
        // FIXME:
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut &mut bool: Encode);
        #[cfg(feature = "alloc")]
        #[cfg(not(any(feature = "unsafe-optimizations", feature = "unstable-refs")))]
        assert_not_impl_any!(Box<&mut bool>: Encode);
        #[cfg(not(any(
            feature = "unsafe-optimizations",
            all(feature = "alloc", feature = "unstable-refs"),
        )))]
        assert_not_impl_any!(&mut [&mut bool]: Encode);
        // FIXME:
        //#[cfg(feature = "alloc")]
        //#[cfg(not(any(feature = "unsafe-optimizations", feature = "unstable-refs")))]
        //assert_not_impl_any!(Box<[&mut bool]>: Encode);
        //#[cfg(feature = "alloc")]
        //#[cfg(not(any(feature = "unsafe-optimizations", feature = "unstable-refs")))]
        //assert_not_impl_any!(Vec<[&mut bool]>: Encode);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!([&mut bool; 2]: Encode);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(Option<[&mut bool]>: Encode);
    }
}
