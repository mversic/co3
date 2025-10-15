//! Logic related to the conversion of IR types to equivalent robust C types. Types that are mapped into
//! one of the predefined [`Ir`] types will be provided an automatic implementation of traits in this module.
//!
//! Traits in this module mainly exist to bridge the gap between IR and C type equivalents. User should
//! only implement these traits if none of the predefined IR types provide an adequate mapping.
use alloc::{boxed::Box, vec::Vec};
use core::ptr::addr_of_mut;

use crate::{
    Decode, Encode, Result, assert_arr_has_non_zero_len,
    ir::{Ir, Opaque},
    out_ptr::NonLocal,
};

/// Marker for an [`Ir`] type that delegates to the pointed-to type when converting
/// the likes of `&Self` or `&[Self]` into an FFI-compatible representation
///
/// This type clones the pointed-to value to get owned value that has implemented
/// [`ExternC`]. This type therefore uses the store
pub trait Cloned {}

impl<R: Ir<Type: Cloned>> Cloned for &R {}
// FIX: This is WRONG!!
impl<R> Cloned for Box<R> {}
// FIX: This is CORRECT!!
//impl<R: Ir<Type: Cloned>> Cloned for Box<R> {}
impl<R> Cloned for [R] {}
impl<R> Cloned for Vec<R> {}
// TODO: This means there is unnecesary clone?
impl<const N: usize> Cloned for [Opaque; N] {}
impl<R: Ir, const N: usize> Cloned for [R; N] where R::Type: Cloned {}

pub(super) fn default_init_arr<R: Default, const N: usize>() -> [R; N] {
    assert_arr_has_non_zero_len::<N>();

    let vec = core::iter::repeat_with(Default::default)
        .take(N)
        .collect::<Vec<_>>();

    // SAFETY: Vec<T> length is N
    unsafe { TryFrom::try_from(vec).unwrap_unchecked() }
}

/// Write a rust value into an out-pointer of any type that doesn't return a
/// reference to the store during serialization into an FFI-compatible type
///
/// # Safety
///
/// out-pointer must be valid for writes
pub unsafe fn write_non_local<'itm, R: Ir<Type = S> + NonLocal + Encode<'itm> + 'itm, S: 'itm>(
    source: R,
    out_ptr: *mut R::CType,
) {
    let mut store = Default::default();

    unsafe {
        // NOTE: Bypasses the erroneous lifetime check.
        // Correct as long as `R::encode` doesn't return a reference to the store (`R: NonLocal`)
        let store_borrow = &mut *addr_of_mut!(store);
        out_ptr.write(Encode::encode(source, store_borrow));
    }
}

/// Read a rust value from an out-pointer of any type that doesn't return a reference
/// reference to the store during serialization from an FFI-compatible type
///
/// # Errors
///
/// Check [`Decode::decode`]
///
/// # Safety
///
/// Check [`Decode::decode`]
pub unsafe fn read_non_local<'itm, R: Ir<Type = S> + NonLocal + Decode<'itm> + 'itm, S: 'itm>(
    out_ptr: R::CType,
) -> Result<R> {
    let mut store = Default::default();

    unsafe {
        // NOTE: Bypasses the erroneous lifetime check.
        // Correct as long as `R::decode` doesn't return a reference to the store (`R: NonLocal`)
        let store_borrow = &mut *addr_of_mut!(store);
        Decode::decode(out_ptr, store_borrow)
    }
}
