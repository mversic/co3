// Reverse a borrowed, fixed-size group of four nonzero bytes.
include!("../support/mod.rs");

use core::num::NonZeroU8;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn update(input: &mut [NonZeroU8; 4]) {
    input.reverse();
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    use core::num::NonZeroU8;

    pub fn update(input: &mut [NonZeroU8; 4]) {
        input.reverse();
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn update(#[soft] input: &mut [NonZeroU8; 4]);
}

fn main() {}
