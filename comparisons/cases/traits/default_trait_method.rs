// Exporting an existing concrete implementation of a trait with a default method.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
pub trait Named: Send + Sync {
    fn name(&self) -> u32 {
        42
    }
}

#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec))]
#[cfg_attr(feature = "co3", rust_spec(opaque))]
pub struct Item {
    pub input: u32,
}

impl Named for Item {}

#[cfg(any(feature = "diplomat", feature = "interoptopus", feature = "safer_ffi"))]
compile_error!("This fixture has no trait-export adapter for the selected framework.");

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    type Item;

    impl Named for Item {
        fn name(&self) -> u32;
    }
}

fn main() {}
