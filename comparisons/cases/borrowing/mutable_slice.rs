// Mutable slices of elements other than u8.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(values: &mut [u32]) {
    values.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn fill(values: &mut [u32]) {
        values.fill(42);
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(values: &mut [u32]);
}

fn main() {}
