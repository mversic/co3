// A reference nested inside Option.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn read(input: Option<&u32>) -> u32 {
    input.copied().unwrap_or_default()
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn read(input: Option<&u32>) -> u32 {
        input.copied().unwrap_or_default()
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn read(input: Option<&u32>) -> u32;
}

fn main() {}
