// Positive control: ordinary by-input aliases work in both backends.
include!("../support/mod.rs");

pub type Byte = u8;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn identity(input: Byte) -> Byte {
    input
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub type Byte = u8;

    pub fn identity(input: Byte) -> Byte {
        input
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn identity(input: Byte) -> Byte;
}

fn main() {}
