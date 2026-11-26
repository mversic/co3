// An alias hides String in a by-value parameter.
include!("../support/mod.rs");

pub type Text = String;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn length(input: Text) -> u64 {
    input.len() as u64
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub type Text = String;

    pub fn length(input: Text) -> u64 {
        input.len() as u64
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn length(input: move Text) -> u64;
}

fn main() {}
