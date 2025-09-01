use core::mem::ManuallyDrop;

use disjoint_impls::disjoint_impls;

use super::*;
use crate::ReprC;

/// Marker trait for a type whose [`Transmute::is_valid`] always returns true. The main
/// use of this trait is to guard against the use of `&mut T` in FFI where the caller
/// can set the underlying `T` to a trap representation and cause UB.
///
/// # Safety
///
/// Implementation of [`Transmute::is_valid`] must always return true for this type
pub unsafe trait InfallibleTransmute {}

disjoint_impls! {
    /// Marker trait for a type that can be transmuted into some C Type
    ///
    /// # Safety
    ///
    /// * `Self` and `Self::Target` must be mutually transmutable
    /// * `Self::is_valid` must not return false positives, i.e. return `true` for trap representations
    pub unsafe trait Transmute {
        /// Type that [`Self`] can be transmuted into
        type Target;

        /// Function that is called when transmuting types to check for trap representations. This function
        /// will never return false positives, i.e. return `true` for a trap representations.
        ///
        /// # Safety
        ///
        /// Any raw pointer in [`Self::Target`] that will be dereferenced must be valid.
        fn is_valid(target: &Self::Target) -> bool;
    }

    // SAFETY: Transmuting a reference to a pointer of the same type
    unsafe impl<R: ReprC> Transmute for &R where R: Ir<Type = Robust> {
        type Target = *const R;

        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    // SAFETY: Transmuting a reference to a pointer of the same type
    unsafe impl<R> Transmute for &R where R: Ir<Type = Opaque> {
        type Target = *const R;

        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    // SAFETY: Transmute relation is transitive
    unsafe impl<'itm, R: Transmute> Transmute for &'itm R where R: Ir<Type = Transparent> {
        type Target = &'itm <R>::Target;

        fn is_valid(target: &Self::Target) -> bool {
            <R>::is_valid(target)
        }
    }

    // SAFETY: Transmuting a reference to a pointer of the same type
    unsafe impl<R: ReprC> Transmute for &mut R where R: Ir<Type = Robust> {
        type Target = *mut R;

        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    // SAFETY: Transmuting a reference to a pointer of the same type
    unsafe impl<R> Transmute for &mut R where R: Ir<Type = Opaque> {
        type Target = *mut R;

        fn is_valid(target: &Self::Target) -> bool {
            !target.is_null()
        }
    }
    // SAFETY: Transmute relation is transitive
    unsafe impl<'itm, R: Transmute> Transmute for &'itm mut R where R: Ir<Type = Transparent> {
        type Target = &'itm mut <R>::Target;

        fn is_valid(target: &Self::Target) -> bool {
            <R>::is_valid(target)
        }
    }

    // SAFETY: Robust arrays have a defined representation
    unsafe impl<R: ReprC, const N: usize> Transmute for [R; N] where R: Ir<Type = Robust> {
        type Target = [R; N];

        fn is_valid(_: &Self::Target) -> bool {
            true
        }
    }
    // SAFETY: Transmute relation is transitive
    unsafe impl<R: Transmute, const N: usize> Transmute for [R; N] where R: Ir<Type = Transparent> {
        type Target = [<R>::Target; N];

        fn is_valid(target: &Self::Target) -> bool {
            target.iter().all(|elem| <R>::is_valid(elem))
        }
    }
}

#[repr(C)]
union TransmuteHelper<R: Transmute> {
    source: ManuallyDrop<R>,
    target: ManuallyDrop<R::Target>,
}

// SAFETY: Arrays have a defined representation
unsafe impl<R: InfallibleTransmute, const N: usize> InfallibleTransmute for [R; N] {}

pub(super) fn transmute_into_target<R: Transmute>(source: R) -> R::Target {
    let transmute_helper = TransmuteHelper {
        source: ManuallyDrop::new(source),
    };

    // SAFETY: Transmute is always valid because R::Target is a superset of R
    ManuallyDrop::into_inner(unsafe { transmute_helper.target })
}
pub(super) unsafe fn transmute_from_target<R: Transmute>(source: R::Target) -> Result<R> {
    if !R::is_valid(&source) {
        return Err(FfiReturn::TrapRepresentation);
    }

    let transmute_helper = TransmuteHelper {
        target: ManuallyDrop::new(source),
    };

    Ok(ManuallyDrop::into_inner(unsafe { transmute_helper.source }))
}

pub(super) fn transmute_into_target_box<R: Transmute>(source: Box<R>) -> Box<R::Target> {
    // SAFETY: `R` is guaranteed to be transmutable into `R::Target`
    unsafe { Box::from_raw(Box::into_raw(source).cast::<R::Target>()) }
}
pub(super) unsafe fn transmute_from_target_box<R: Transmute>(
    source: Box<R::Target>,
) -> Result<Box<R>> {
    if !R::is_valid(&source) {
        return Err(FfiReturn::TrapRepresentation);
    }

    Ok(unsafe { Box::from_raw(Box::into_raw(source).cast::<R>()) })
}

#[allow(clippy::boxed_local)]
pub(super) fn transmute_into_target_boxed_slice<R: Transmute>(
    mut source: Box<[R]>,
) -> Box<[R::Target]> {
    let (ptr, len) = (source.as_mut_ptr().cast::<R::Target>(), source.len());
    // SAFETY: `R` is guaranteed to be transmutable into `R::Target`
    unsafe { Box::from_raw(core::slice::from_raw_parts_mut(ptr, len)) }
}
#[allow(clippy::boxed_local)]
pub(super) unsafe fn transmute_from_target_boxed_slice<R: Transmute>(
    mut source: Box<[R::Target]>,
) -> Result<Box<[R]>> {
    if !source.iter().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    Ok(unsafe {
        Box::from_raw(core::slice::from_raw_parts_mut(
            source.as_mut_ptr().cast(),
            source.len(),
        ))
    })
}

pub(super) fn transmute_into_target_ref_slice<R: Transmute>(source: &[R]) -> &[R::Target] {
    let (ptr, len) = (source.as_ptr().cast::<R::Target>(), source.len());
    // SAFETY: `R` is guaranteed to be transmutable into `R::Target`
    unsafe { core::slice::from_raw_parts(ptr, len) }
}
pub(super) unsafe fn transmute_from_target_ref_slice<R: Transmute>(
    source: &[R::Target],
) -> Result<&[R]> {
    if !source.iter().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    Ok(unsafe { core::slice::from_raw_parts(source.as_ptr().cast(), source.len()) })
}

pub(super) fn transmute_into_target_slice_mut<R: Transmute>(source: &mut [R]) -> &mut [R::Target] {
    let (ptr, len) = (source.as_mut_ptr().cast::<R::Target>(), source.len());
    // SAFETY: `R` is guaranteed to be transmutable into `R::Target`
    unsafe { core::slice::from_raw_parts_mut(ptr, len) }
}
pub(super) unsafe fn transmute_from_target_slice_mut<R: Transmute>(
    source: &mut [R::Target],
) -> Result<&mut [R]> {
    if !source.iter_mut().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    Ok(unsafe { core::slice::from_raw_parts_mut(source.as_mut_ptr().cast(), source.len()) })
}

pub(super) fn transmute_into_target_vec<R: Transmute>(source: Vec<R>) -> Vec<R::Target> {
    let mut vec = ManuallyDrop::new(source);

    // SAFETY: `Transparency` guarantees `T` can be transmuted into `C`
    unsafe { Vec::from_raw_parts(vec.as_mut_ptr().cast(), vec.len(), vec.capacity()) }
}
pub(super) unsafe fn transmute_from_target_vec<R: Transmute>(
    source: Vec<R::Target>,
) -> Result<Vec<R>> {
    if !source.iter().all(|item| R::is_valid(item)) {
        return Err(FfiReturn::TrapRepresentation);
    }

    let mut vec = ManuallyDrop::new(source);
    Ok(unsafe { Vec::from_raw_parts(vec.as_mut_ptr().cast(), vec.len(), vec.capacity()) })
}
