// An alias defined in a separate dependency crate.
include!("../support/mod.rs");

use comparison_types::Count;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn identity(input: Count) -> Count {
    input
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    use comparison_types::Count;

    pub fn identity(input: Count) -> Count {
        input
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn identity(input: Count) -> Count;
}

fn main() {}
