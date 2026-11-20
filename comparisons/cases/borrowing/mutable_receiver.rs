// An exclusive method receiver without interior-mutability adaptation.
include!("../support/mod.rs");

// BoltFFI requires the caller to guarantee single-threaded access.
#[cfg(not(feature = "safer_ffi"))]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
#[cfg_attr(feature = "boltffi", boltffi::export(single_threaded))]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi(service))]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec))]
#[cfg_attr(feature = "co3", rust_spec(opaque))]
pub struct Counter {
    input: u32,
}

#[cfg(not(feature = "safer_ffi"))]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
impl Counter {
    pub fn increment(&mut self) {
        self.input += 1;
    }
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    #[diplomat::opaque]
    pub struct Counter {
        input: u32,
    }

    impl Counter {
        pub fn increment(&mut self) {
            self.input += 1;
        }
    }
}

#[cfg(feature = "safer_ffi")]
compile_error!("This fixture needs a safer-ffi free-function adapter for the method.");

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    type Counter;

    impl Counter {
        fn increment(&mut self);
    }
}

fn main() {}
