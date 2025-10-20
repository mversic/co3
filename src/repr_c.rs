//! Logic related to the conversion of IR types to equivalent robust C types. Types that are mapped into
//! one of the predefined [`Ir`] types will be provided an automatic implementation of traits in this module.
//!
//! Traits in this module mainly exist to bridge the gap between IR and C type equivalents. User should
//! only implement these traits if none of the predefined IR types provide an adequate mapping.
use alloc::{vec::Vec};

use crate::{
    assert_arr_has_non_zero_len,
};

pub(super) fn default_init_arr<R: Default, const N: usize>() -> [R; N] {
    assert_arr_has_non_zero_len::<N>();

    let vec = core::iter::repeat_with(Default::default)
        .take(N)
        .collect::<Vec<_>>();

    // SAFETY: Vec<T> length is N
    unsafe { TryFrom::try_from(vec).unwrap_unchecked() }
}
