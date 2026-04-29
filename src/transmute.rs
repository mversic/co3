#[cfg(feature = "alloc")]
use alloc_crate::{boxed::Box, vec::Vec};
use core::mem::ManuallyDrop;

use disjoint_impls::disjoint_impls;

#[cfg(feature = "alloc")]
use crate::boxed::CBox;
use crate::{
    DstFamily, EncodeWithStore, ReprC, Sized_, SliceDst, SliceLike, Store,
    assert_arr_has_non_zero_len,
    ir::{Cloned, NonRobust, Opaque, ReprFamily, Robust, Transmuted},
    niche::{NicheFamily, StableNiche, WithNiche, WithStableNiche, WithoutNiche},
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
        type Target = CBox<R>;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_none()
        }
    }
    #[cfg(feature = "alloc")]
    unsafe impl<R: ReprFamily<Kind = Opaque>> CheckedTransmute for Box<R> {
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
        R: CheckedTransmute<Target: ReprFamily<Kind = Robust> + ReprC>,
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
        type Target: ?Sized;

        /// Called when transmuting [`Self::Target`] back into [`Self`] to check for trap representations.
        ///
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::Target) -> bool;
    }

    unsafe impl<R: ?Sized> FlatTransmute for R
    where
        Self: ReprFamily<Kind = Robust>,
    {
        type Target = Self;

        #[inline(always)]
        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }
    unsafe impl<R: ?Sized> FlatTransmute for R
    where
        Self: ReprFamily<Kind = Opaque>,
    {
        type Target = Self;

        #[inline(always)]
        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }
    unsafe impl<R: ?Sized> FlatTransmute for R
    where
        Self: ReprFamily<Kind: Cloned>,
    {
        type Target = Self;

        #[inline(always)]
        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }

    unsafe impl<R: CheckedTransmute<Target: FlatTransmute + Sized>> FlatTransmute for R
    where
        Self: ReprFamily<Kind = Transmuted> + DstFamily<Kind = Sized_>,
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
    unsafe impl<R: ?Sized + CheckedTransmute<Target: FlatTransmute<Target: SliceDst> + SliceDst>> FlatTransmute for R
    where
        Self: ReprFamily<Kind = Transmuted> + DstFamily<Kind = SliceLike>,
    {
        type Target = <R::Target as FlatTransmute>::Target;

        #[inline(always)]
        fn is_valid(target: &Self::Target) -> bool {
            if !<R::Target as FlatTransmute>::is_valid(target) {
                return false;
            }

            let target = unsafe { R::Target::from_raw_parts(target.as_ptr().cast(), target.len()) };
            <R as CheckedTransmute>::is_valid(target)
        }
    }
}

pub struct TransmutedRefMutStore<'a, R> {
    _target: Option<R>,
    _original: Option<&'a mut R>,
}

impl<'a, R> Default for TransmutedRefMutStore<'a, R> {
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
    /// of [`crate::EncodeWithStore`] is provided even for mutable references to non-robust types. Use it
    /// at your own discretion
    ///
    /// # Safety
    ///
    /// The type must not carry a mutable reference to a non-robust type
    pub unsafe trait EncodeTransmuted<const UNSAFE_OPTIMIZATIONS: bool = false>: CheckedTransmute<Target: Sized> + Sized {
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

    unsafe impl<R: ReprFamily<Kind: NonRobust>> EncodeTransmuted<false> for &mut R where Self: CheckedTransmute<Target: EncodeWithStore> {
        type Store = <Self::Target as crate::EncodeWithStore>::Store;
    }
    unsafe impl<R: ReprFamily<Kind = Robust> + NicheFamily<Kind = WithoutNiche> + ReprC> EncodeTransmuted<false> for &mut R where Self: CheckedTransmute<Target = *mut R> {
        type Store = <Self::Target as crate::EncodeWithStore>::Store;
    }
    unsafe impl<'a, R: ReprFamily<Kind = Robust> + NicheFamily<Kind: WithNiche> + ReprC> EncodeTransmuted<false> for &'a mut R where Self: CheckedTransmute<Target = *mut R> {
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

unsafe impl<R> EncodeTransmuted<false> for &R
where
    Self: CheckedTransmute<Target: EncodeWithStore>,
{
    type Store = <Self::Target as crate::EncodeWithStore>::Store;
}
// WARN: Since `Box<&mut R>` is mapped to `*mut *mut R` this can be disputed in the case of
// no ownership transfer where Box's invariant can be violated by the caller by NULLing the
// inner pointer. However, because the box is immediately dropped following the function call,
// we deem it ok as it would most likely lead to a catastrophic segfault, not a silent UB.
#[cfg(feature = "alloc")]
unsafe impl<R: EncodeTransmuted> EncodeTransmuted<false> for Box<R>
where
    Self: CheckedTransmute<Target: EncodeWithStore>,
{
    type Store = <Self::Target as crate::EncodeWithStore>::Store;

    fn encode_transmuted<'itm>(self, _store: &'itm mut Self::Store) -> Self::Target
    where
        Self: 'itm,
    {
        unimplemented!()
    }
}
unsafe impl<R> EncodeTransmuted<false> for Option<R>
where
    Self: CheckedTransmute<Target: EncodeWithStore>,
{
    type Store = <Self::Target as crate::EncodeWithStore>::Store;
}
unsafe impl<R, const N: usize> EncodeTransmuted<false> for [R; N]
where
    Self: CheckedTransmute<Target: EncodeWithStore>,
{
    type Store = <Self::Target as crate::EncodeWithStore>::Store;
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

// TODO: this is only temporarily public
// https://github.com/mversic/co3/issues/76
#[doc(hidden)]
pub fn transmute_into_target<R: CheckedTransmute<Target: Sized>>(source: R) -> R::Target {
    assert_size_and_allignment_match::<R>();

    let transmute_helper = TransmuteHelper {
        source: ManuallyDrop::new(source),
    };

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    ManuallyDrop::into_inner(unsafe { transmute_helper.target })
}
// TODO: this is only temporarily public
// https://github.com/mversic/co3/issues/76
#[doc(hidden)]
pub fn transmute_from_target<R: CheckedTransmute<Target: Sized>>(source: R::Target) -> Option<R> {
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

pub(super) fn transmute_into_target_ref_dst<
    R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst>,
>(
    source: &R,
) -> &R::Target {
    let (ptr, len) = (source.as_ptr().cast(), source.len());
    unsafe { R::Target::from_raw_parts(ptr, len) }
}
pub(super) fn transmute_from_target_ref_dst<
    R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst>,
>(
    source: &R::Target,
) -> Option<&R> {
    if !R::is_valid(source) {
        return None;
    }

    let (ptr, len) = (source.as_ptr().cast(), source.len());
    Some(unsafe { R::from_raw_parts(ptr, len) })
}

pub(super) fn transmute_into_target_dst_mut<
    R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst>,
>(
    source: &mut R,
) -> &mut R::Target {
    let (ptr, len) = (source.as_mut_ptr().cast(), source.len());
    unsafe { R::Target::from_raw_parts_mut(ptr, len) }
}
pub(super) fn transmute_from_target_dst_mut<
    R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst>,
>(
    source: &mut R::Target,
) -> Option<&mut R> {
    if !R::is_valid(source) {
        return None;
    }

    let (ptr, len) = (source.as_mut_ptr().cast(), source.len());
    Some(unsafe { R::from_raw_parts_mut(ptr, len) })
}

#[cfg(feature = "alloc")]
pub(super) fn transmute_into_target_boxed_dst<
    R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst>,
>(
    source: Box<R>,
) -> Box<R::Target> {
    let mut source = ManuallyDrop::new(source);
    let (ptr, len) = (source.as_mut_ptr().cast(), source.len());
    unsafe { Box::from_raw(R::Target::from_raw_parts_mut(ptr, len)) }
}
#[cfg(feature = "alloc")]
pub(super) fn transmute_from_target_boxed_dst<
    R: ?Sized + SliceDst + CheckedTransmute<Target: SliceDst>,
>(
    source: Box<R::Target>,
) -> Option<Box<R>> {
    if !R::is_valid(&source) {
        return None;
    }

    let mut source = ManuallyDrop::new(source);

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Some(unsafe {
        Box::from_raw(R::from_raw_parts_mut(
            source.as_mut_ptr().cast(),
            source.len(),
        ))
    })
}

#[cfg(feature = "alloc")]
pub(super) fn transmute_into_target_vec<R: CheckedTransmute<Target: Sized>>(
    source: Vec<R>,
) -> Vec<R::Target> {
    assert_size_and_allignment_match::<R>();

    let mut vec = ManuallyDrop::new(source);

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { Vec::from_raw_parts(vec.as_mut_ptr().cast(), vec.len(), vec.capacity()) }
}
#[cfg(feature = "alloc")]
pub(super) fn transmute_from_target_vec<R: CheckedTransmute<Target: Sized>>(
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

fn assert_size_and_allignment_match<R: CheckedTransmute<Target: Sized>>() {
    const {
        debug_assert!(core::mem::size_of::<R>() == core::mem::size_of::<R::Target>());
        debug_assert!(core::mem::align_of::<R>() == core::mem::align_of::<R::Target>());
    };
}

#[cfg(test)]
mod tests {
    use static_assertions::assert_impl_all;

    use super::*;
    #[cfg(feature = "alloc")]
    use crate::boxed::CBoxedSlice;
    use crate::{
        DecodeWithStore, EncodeWithStore, ExternC,
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
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut u8>,
            DecodeWithStore<'static>,
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<bool>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut u8>,
        //    DecodeWithStore<'static>,
        //    EncodeWithStore,
        //);
        assert_impl_all!(&[bool]:
            ReprFamily<Kind = &'static Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut [bool]:
            ReprFamily<Kind = &'static mut Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[bool]>:
            ReprFamily<Kind = Box<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<bool>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<u8>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!([bool; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [u8; 2]>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(Option<bool>:
            ReprFamily<Kind = Option<WithCustomNiche>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        // FIXME:
        //#[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        //assert_impl_all!(&mut bool: EncodeWithStore);
        //#[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        //assert_impl_all!(&mut [bool]: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut bool: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut [bool]: EncodeWithStore);
    }

    #[test]
    fn robust_ref() {
        assert_impl_all!(&&u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *const u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut &u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *const u8>,
            DecodeWithStore<'static>,
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<&bool>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut *const u8>,
        //    DecodeWithStore<'static>,
        //    EncodeWithStore,
        //);
        assert_impl_all!(&[&u8]:
            ReprFamily<Kind = &'static Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*const u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut [&u8]:
            ReprFamily<Kind = &'static mut Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const u8>>,
            DecodeWithStore<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&u8]>:
            ReprFamily<Kind = Box<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&u8>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!([&u8; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*const u8; 2]>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(Option<&u8>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *const u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        // FIXME:
        //#[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        //assert_impl_all!(&mut &u8: EncodeWithStore);
        //#[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        //assert_impl_all!(&mut [&u8]: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut &u8: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut [&u8]: EncodeWithStore);
    }

    #[test]
    fn transparent_ref() {
        assert_impl_all!(&&bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *const u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut &bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *const u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<&bool>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut *const u8>,
        //    DecodeWithStore<'static>,
        //    EncodeWithStore,
        //);
        assert_impl_all!(&[&bool]:
            ReprFamily<Kind = &'static Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*const u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut [&bool]:
            ReprFamily<Kind = &'static mut Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*const u8>>,
            DecodeWithStore<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&bool]>:
            ReprFamily<Kind = Box<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&bool>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*const u8>>,
            // FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!([&bool; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*const u8; 2]>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(Option<&bool>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *const u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        // FIXME:
        //#[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        //assert_impl_all!(&mut &bool: EncodeWithStore);
        //#[cfg(any(feature = "unsafe-optimizations", feature = "alloc")]
        //assert_impl_all!(&mut [&bool]: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut &bool: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut [&bool]: EncodeWithStore);
    }

    #[test]
    fn robust_ref_mut() {
        assert_impl_all!(&&mut u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *mut u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut &mut u8:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *mut u8>,
            DecodeWithStore<'static>,
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<&mut u8>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut *mut u8>,
        //    DecodeWithStore<'static>,
        //    EncodeWithStore,
        //);
        assert_impl_all!(&[&mut u8]:
            ReprFamily<Kind = &'static Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*mut u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut [&mut u8]:
            ReprFamily<Kind = &'static mut Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*mut u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut u8]>:
            ReprFamily<Kind = Box<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            //FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut u8>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            //FIXME:
            //DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!([&mut u8; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*mut u8; 2]>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(Option<&mut u8>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *mut u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );

        // FIXME:
        //#[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        //assert_impl_all!(&mut &mut u8: EncodeWithStore);
        //#[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        //assert_impl_all!(&mut [&mut u8]: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut &mut u8: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut [&mut u8]: EncodeWithStore);
    }

    #[test]
    fn transparent_ref_mut() {
        assert_impl_all!(&&mut bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *const *mut u8>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut &mut bool:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithStableNiche>,
            StableNiche<CType = *mut *mut u8>,
            DecodeWithStore<'static>,
        );
        // FIXME:
        //#[cfg(feature = "alloc")]
        //assert_impl_all!(Box<&mut bool>:
        //    ReprFamily<Kind = Transmuted>,
        //    NicheFamily<Kind = WithStableNiche>,
        //    StableNiche<CType = *mut *mut u8>,
        //    DecodeWithStore<'static>,
        //);
        assert_impl_all!(&[&mut bool]:
            ReprFamily<Kind = &'static Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSlice<*mut u8>>,
            DecodeWithStore<'static>,
            EncodeWithStore,
        );
        assert_impl_all!(&mut [&mut bool]:
            ReprFamily<Kind = &'static mut Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CSliceMut<*mut u8>>,
            DecodeWithStore<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Box<[&mut bool]>:
            ReprFamily<Kind = Box<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            // FIXME:
            //DecodeWithStore<'static>,
        );
        #[cfg(feature = "alloc")]
        assert_impl_all!(Vec<&mut bool>:
            ReprFamily<Kind = Vec<Transmuted>>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = CBoxedSlice<*mut u8>>,
            // FIXME:
            //DecodeWithStore<'static>,
        );
        assert_impl_all!([&mut bool; 2]:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithCustomNiche>,
            Niche<CType = [*mut u8; 2]>,
            DecodeWithStore<'static>,
        );
        assert_impl_all!(Option<&mut bool>:
            ReprFamily<Kind = Transmuted>,
            NicheFamily<Kind = WithoutNiche>,
            ExternC<CType = *mut u8>,
            DecodeWithStore<'static>,
        );

        #[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        assert_impl_all!(&mut &mut bool: EncodeWithStore);

        //#[cfg(feature = "alloc")]
        //#[cfg(feature = "unsafe-optimizations")]
        //assert_impl_all!(Box<&mut bool>: EncodeWithStore);
        #[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        assert_impl_all!(&mut [&mut bool]: EncodeWithStore);
        #[cfg(feature = "alloc")]
        #[cfg(feature = "unsafe-optimizations")]
        assert_impl_all!(Box<[&mut bool]>: EncodeWithStore);
        #[cfg(feature = "alloc")]
        #[cfg(feature = "unsafe-optimizations")]
        assert_impl_all!(Vec<&mut bool>: EncodeWithStore);
        #[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        assert_impl_all!([&mut bool; 2]: EncodeWithStore);
        #[cfg(any(feature = "unsafe-optimizations", feature = "alloc"))]
        assert_impl_all!(Option<&mut bool>: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut &mut bool: EncodeWithStore);
        //#[cfg(feature = "alloc")]
        //#[cfg(not(any(feature = "unsafe-optimizations", feature = "unstable-refs")))]
        //assert_not_impl_any!(Box<&mut bool>: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(&mut [&mut bool]: EncodeWithStore);
        // FIXME:
        //#[cfg(feature = "alloc")]
        //#[cfg(not(any(feature = "unsafe-optimizations", feature = "unstable-refs")))]
        //assert_not_impl_any!(Box<[&mut bool]>: EncodeWithStore);
        //#[cfg(feature = "alloc")]
        //#[cfg(not(any(feature = "unsafe-optimizations", feature = "unstable-refs")))]
        //assert_not_impl_any!(Vec<[&mut bool]>: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!([&mut bool; 2]: EncodeWithStore);
        //#[cfg(not(any(
        //    feature = "unsafe-optimizations",
        //    all(feature = "alloc", feature = "unstable-refs"),
        //)))]
        //assert_not_impl_any!(Option<[&mut bool]>: EncodeWithStore);
    }
}
