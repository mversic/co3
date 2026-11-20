// CO3 exports a concrete trait implementation, not a foreign-implementable trait.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
pub trait Source: Send + Sync {
    type Item;
    fn get(&self) -> Self::Item;
}

#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec))]
#[cfg_attr(feature = "co3", rust_spec(opaque))]
pub struct Counter {
    pub input: u32,
}

impl Source for Counter {
    type Item = u32;
    fn get(&self) -> u32 {
        42
    }
}

#[cfg(any(feature = "diplomat", feature = "interoptopus", feature = "safer_ffi"))]
compile_error!("This fixture has no trait-export adapter for the selected framework.");

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    type Counter;

    impl Source for Counter {
        fn get(&self) -> u32;
    }
}

fn main() {}
