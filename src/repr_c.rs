//! Logic related to the conversion of IR types to equivalent robust C types. Types that are mapped into
//! one of the predefined [`Ir`] types will be provided an automatic implementation of traits in this module.
//!
//! Traits in this module mainly exist to bridge the gap between IR and C type equivalents. User should
//! only implement these traits if none of the predefined IR types provide an adequate mapping.
use alloc::{boxed::Box, vec::Vec};

use crate::{
    Encode, assert_arr_has_non_zero_len,
    ir::{Ir, Opaque},
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
