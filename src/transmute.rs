use core::mem::ManuallyDrop;

use disjoint_impls::disjoint_impls;

use super::*;
use crate::ReprC;

/// Marker trait for a type that can be **safely transmuted** into another type for all values.
///
/// # Safety
///
/// * `Self` and `Self::Target` must be mutually transmutable
/// * `Self::is_valid` must not return false positives, i.e. return `true` for trap representations
pub unsafe trait Transmute {
    /// Type that [`Self`] can be transmuted into
    type Target;
    /// Called when transmuting [`Self::Target`] into [`Self`] to check for trap representations.
    /// This function must never return false positives, i.e. return `true` for a trap representation.
    fn is_valid(target: &Self::Target) -> bool;
}
#[allow(clippy::needless_lifetimes)]
const _: () = {
    /// Marker trait for a type that can be **safely transmuted** into another type for all values.
    ///
    /// # Safety
    ///
    /// * `Self` and `Self::Target` must be mutually transmutable
    /// * `Self::is_valid` must not return false positives, i.e. return `true` for trap representations
    pub unsafe trait Transmute0<_TŠČ0: ?Sized> {
        /// Type that [`Self`] can be transmuted into
        type Target;
        /// Called when transmuting [`Self::Target`] into [`Self`] to check for trap representations.
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::Target) -> bool;
    }
    /// Marker trait for a type that can be **safely transmuted** into another type for all values.
    ///
    /// # Safety
    ///
    /// * `Self` and `Self::Target` must be mutually transmutable
    /// * `Self::is_valid` must not return false positives, i.e. return `true` for trap representations
    pub unsafe trait Transmute2<_TŠČ0: ?Sized> {
        /// Type that [`Self`] can be transmuted into
        type Target;
        /// Called when transmuting [`Self::Target`] into [`Self`] to check for trap representations.
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::Target) -> bool;
    }
    /// Marker trait for a type that can be **safely transmuted** into another type for all values.
    ///
    /// # Safety
    ///
    /// * `Self` and `Self::Target` must be mutually transmutable
    /// * `Self::is_valid` must not return false positives, i.e. return `true` for trap representations
    pub unsafe trait Transmute4<_TŠČ0: ?Sized> {
        /// Type that [`Self`] can be transmuted into
        type Target;
        /// Called when transmuting [`Self::Target`] into [`Self`] to check for trap representations.
        /// This function must never return false positives, i.e. return `true` for a trap representation.
        fn is_valid(target: &Self::Target) -> bool;
    }
    unsafe impl<'_lšč0, R: Ir<Type = Robust> + ReprC> Transmute0<Robust>
    for &'_lšč0 R {
        type Target = *const R;
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<'_lšč0, R: Ir<Type = Opaque>> Transmute0<Opaque> for &'_lšč0 R {
        type Target = *const R;
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<'itm, R: Ir<Type = Transparent> + Transmute> Transmute0<Transparent>
    for &'itm R {
        type Target = &'itm <R>::Target;
        fn is_valid(target: &Self::Target) -> bool {
            <R>::is_valid(target)
        }
    }
    unsafe impl<'_lšč0, R: ReprC, const N: usize> Transmute0<[Robust; N]>
    for &'_lšč0 [R; N]
    where
        [R; N]: Ir<Type = [Robust; N]>,
    {
        type Target = *const [R; N];
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<'_lšč0, R: Ir<Type = Robust> + ReprC> Transmute2<Robust>
    for &'_lšč0 mut R {
        type Target = *mut R;
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<'_lšč0, R: Ir<Type = Opaque>> Transmute2<Opaque> for &'_lšč0 mut R {
        type Target = *mut R;
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<'itm, R: Ir<Type = Transparent> + Transmute> Transmute2<Transparent>
    for &'itm mut R {
        type Target = &'itm mut <R>::Target;
        fn is_valid(target: &Self::Target) -> bool {
            <R>::is_valid(target)
        }
    }
    unsafe impl<'_lšč0, R: ReprC, const N: usize> Transmute2<[Robust; N]>
    for &'_lšč0 mut [R; N]
    where
        [R; N]: Ir<Type = [Robust; N]>,
    {
        type Target = *mut [R; N];
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<
        R: Ir<Type = Transparent> + Transmute,
        const N: usize,
    > Transmute4<Transparent> for [R; N] {
        type Target = [<R>::Target; N];
        fn is_valid(target: &Self::Target) -> bool {
            assert_arr_has_non_zero_len::<N>();
            target.iter().all(|elem| <R>::is_valid(elem))
        }
    }
    unsafe impl<R: ReprC, const N: usize> Transmute4<Box<Robust>> for [Box<R>; N]
    where
        Box<R>: Ir<Type = Box<Robust>>,
    {
        type Target = *mut [Box<R>; N];
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<'_lšč0, R: '_lšč0> Transmute for &'_lšč0 R
    where
        Self: Transmute0<<R as Ir>::Type>,
        R: Ir,
    {
        type Target = <Self as Transmute0<<R as Ir>::Type>>::Target;
        fn is_valid(target: &Self::Target) -> bool {
            <Self as Transmute0<<R as Ir>::Type>>::is_valid(target)
        }
    }
    unsafe impl<'_lšč0, R: ReprC> Transmute0<Box<Robust>> for &'_lšč0 Box<R>
    where
        &'_lšč0 Box<R>: Ir<Type = Box<Robust>>,
    {
        type Target = *const Box<R>;
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<'_lšč0, R: '_lšč0> Transmute for &'_lšč0 mut R
    where
        Self: Transmute2<<R as Ir>::Type>,
        R: Ir,
    {
        type Target = <Self as Transmute2<<R as Ir>::Type>>::Target;
        fn is_valid(target: &Self::Target) -> bool {
            <Self as Transmute2<<R as Ir>::Type>>::is_valid(target)
        }
    }
    unsafe impl<'_lšč0, R: ReprC> Transmute2<Box<Robust>> for &'_lšč0 mut Box<R>
    where
        &'_lšč0 mut Box<R>: Ir<Type = Box<Robust>>,
    {
        type Target = *mut Box<R>;
        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    unsafe impl<R, const N: usize> Transmute for [R; N]
    where
        Self: Transmute4<<R as Ir>::Type>,
        R: Ir,
    {
        type Target = <Self as Transmute4<<R as Ir>::Type>>::Target;
        fn is_valid(target: &Self::Target) -> bool {
            <Self as Transmute4<<R as Ir>::Type>>::is_valid(target)
        }
    }
};

/// Marker trait for a type whose [`Transmute::is_valid`] always returns true.
///
/// Main use of this trait is to guard against the use of `&mut T` in FFI where
/// the caller can set the underlying `T` to a trap representation and cause UB.
///
/// # Safety
///
/// Implementation of [`Transmute::is_valid`] must always return true for this type.
pub unsafe trait InfallibleTransmute: Transmute {}

// SAFETY: Array is just a contiguous block of bytes in memory
// and has a defined representation if the element type does
unsafe impl<R: Ir<Type = Transparent> + InfallibleTransmute, const N: usize> InfallibleTransmute
    for [R; N]
{
}

#[repr(C)]
union TransmuteHelper<R: Transmute> {
    source: ManuallyDrop<R>,
    target: ManuallyDrop<R::Target>,
}

pub(super) fn transmute_into_target<R: Transmute>(source: R) -> R::Target {
    let transmute_helper = TransmuteHelper {
        source: ManuallyDrop::new(source),
    };

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    ManuallyDrop::into_inner(unsafe { transmute_helper.target })
}
pub(super) fn transmute_from_target<R: Transmute>(source: R::Target) -> Result<R> {
    if !R::is_valid(&source) {
        return Err(FfiReturn::TrapRepresentation);
    }

    let transmute_helper = TransmuteHelper {
        target: ManuallyDrop::new(source),
    };

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(ManuallyDrop::into_inner(unsafe { transmute_helper.source }))
}

pub(super) fn transmute_into_target_box<R: Transmute>(source: Box<R>) -> Box<R::Target> {
    unsafe { Box::from_raw(Box::into_raw(source).cast::<R::Target>()) }
}
pub(super) fn transmute_from_target_box<R: Transmute>(source: Box<R::Target>) -> Result<Box<R>> {
    if !R::is_valid(&source) {
        return Err(FfiReturn::TrapRepresentation);
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(unsafe { Box::from_raw(Box::into_raw(source).cast::<R>()) })
}

pub(super) fn transmute_into_target_boxed_slice<R: Transmute>(
    #[expect(clippy::boxed_local)] mut source: Box<[R]>,
) -> Box<[R::Target]> {
    let (ptr, len) = (source.as_mut_ptr().cast::<R::Target>(), source.len());

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { Box::from_raw(core::slice::from_raw_parts_mut(ptr, len)) }
}
pub(super) fn transmute_from_target_boxed_slice<R: Transmute>(
    #[expect(clippy::boxed_local)] mut source: Box<[R::Target]>,
) -> Result<Box<[R]>> {
    if !source.iter().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(unsafe {
        Box::from_raw(core::slice::from_raw_parts_mut(
            source.as_mut_ptr().cast(),
            source.len(),
        ))
    })
}

pub(super) fn transmute_into_target_ref_slice<R: Transmute>(source: &[R]) -> &[R::Target] {
    let (ptr, len) = (source.as_ptr().cast::<R::Target>(), source.len());
    unsafe { core::slice::from_raw_parts(ptr, len) }
}
pub(super) fn transmute_from_target_ref_slice<R: Transmute>(source: &[R::Target]) -> Result<&[R]> {
    if !source.iter().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(unsafe { core::slice::from_raw_parts(source.as_ptr().cast(), source.len()) })
}

pub(super) fn transmute_into_target_slice_mut<R: Transmute>(source: &mut [R]) -> &mut [R::Target] {
    let (ptr, len) = (source.as_mut_ptr().cast::<R::Target>(), source.len());

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { core::slice::from_raw_parts_mut(ptr, len) }
}
pub(super) fn transmute_from_target_slice_mut<R: Transmute>(
    source: &mut [R::Target],
) -> Result<&mut [R]> {
    if !source.iter_mut().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(unsafe { core::slice::from_raw_parts_mut(source.as_mut_ptr().cast(), source.len()) })
}

pub(super) fn transmute_into_target_vec<R: Transmute>(source: Vec<R>) -> Vec<R::Target> {
    let mut vec = ManuallyDrop::new(source);

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    unsafe { Vec::from_raw_parts(vec.as_mut_ptr().cast(), vec.len(), vec.capacity()) }
}
pub(super) fn transmute_from_target_vec<R: Transmute>(source: Vec<R::Target>) -> Result<Vec<R>> {
    if !source.iter().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    let mut vec = ManuallyDrop::new(source);

    // SAFETY: Soundness is guaranteed by [`Transmute`]
    Ok(unsafe { Vec::from_raw_parts(vec.as_mut_ptr().cast(), vec.len(), vec.capacity()) })
}
