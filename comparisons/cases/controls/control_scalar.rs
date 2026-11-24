// Positive control: a plain scalar export.
include!("../support/mod.rs");

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn increment(input: u32) -> u32 {
    input + 1
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn increment(input: u32) -> u32 {
        input + 1
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]
    fn increment(input: u32) -> u32;
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(increment: (u32) -> u32);

fn main() {}
