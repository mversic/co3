// Exclusive access permits reading and updating the contained nonzero byte.
include!("../support/mod.rs");

use core::num::NonZeroU8;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn update(input: &mut core::cell::UnsafeCell<NonZeroU8>) {
    *input.get_mut() = NonZeroU8::MIN;
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    use core::num::NonZeroU8;

    pub fn update(input: &mut core::cell::UnsafeCell<NonZeroU8>) {
        *input.get_mut() = NonZeroU8::MIN;
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn update(#[soft] input: &mut core::cell::UnsafeCell<NonZeroU8>);
}

fn main() {}
