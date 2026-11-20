// Async export is a capability test, not an equivalent-type spelling test.
include!("../support/mod.rs");

#[cfg(not(feature = "safer_ffi"))]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
pub async fn answer() -> u32 {
    42
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub async fn answer() -> u32 {
        42
    }
}

#[cfg(feature = "safer_ffi")]
compile_error!("This fixture needs a safer-ffi async executor and ffi_await! adapter.");

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]
    async fn answer() -> u32;
}

fn main() {}
