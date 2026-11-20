// A qualified impl self-type; the object itself is otherwise supported.
include!("../support/mod.rs");

pub mod model {
    #[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
    #[cfg_attr(feature = "boltffi", boltffi::export)]
    #[cfg_attr(feature = "interoptopus", interoptopus::ffi(service))]
    #[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec))]
    #[cfg_attr(feature = "co3", rust_spec(opaque))]
    pub struct Widget {
        pub input: u32,
    }
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
impl model::Widget {
    pub fn answer(&self) -> u32 {
        42
    }
}

#[cfg(any(feature = "co3", feature = "interoptopus"))]
use model::Widget;

#[cfg(feature = "diplomat")]
compile_error!("This fixture needs a Diplomat bridge for the nested model::Widget impl.");

#[cfg(feature = "safer_ffi")]
compile_error!("This fixture needs a safer-ffi free-function adapter for the method.");

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    type Widget;

    impl model::Widget {
        fn answer(&self) -> u32;
    }
}

fn main() {}
