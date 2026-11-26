// Export increment(&mut self) on a record whose field is visible to the caller.

include!("../support/mod.rs");

#[cfg(not(any(feature = "diplomat", feature = "safer_ffi")))]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "boltffi", boltffi::data)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Counter {
    pub input: u32,
}

#[cfg(not(any(feature = "diplomat", feature = "safer_ffi")))]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::data(impl))]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
impl Counter {
    pub fn increment(&mut self) {
        self.input += 1;
    }
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {

    #[diplomat::attr(auto, mut_struct_ref)]
    #[repr(C)]
    pub struct Counter {
        pub input: u32,
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

    impl Counter {
        fn increment(&mut self);
    }
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "mutable_receiver__Counter__increment"]
        fn imported_increment(counter: &mut Counter);
    }

    validate(imported_increment);
}

#[cfg(any(feature = "uniffi", feature = "interoptopus", feature = "safer_ffi"))]
fn main() {}

#[cfg(feature = "diplomat")]
fn main() {
    unsafe extern "C" {
        #[link_name = "Counter_increment"]
        fn exported_increment(counter: &mut TestCounter);
    }
    validate(|counter| {
        unsafe { exported_increment(counter) };
    });
}

#[cfg(feature = "boltffi")]
fn main() {
    validate(|counter| {
        // BoltFFI returns the updated record through a separate output pointer.
        let status = unsafe {
            boltffi_method_record_16mutable_receiver7Counter9increment(*counter, counter)
        };
        assert!(status.is_ok());
    });
}

#[cfg(feature = "diplomat")]
type TestCounter = diplomat_case::Counter;
#[cfg(not(any(feature = "diplomat", feature = "safer_ffi")))]
type TestCounter = Counter;

#[cfg(not(feature = "safer_ffi"))]
fn validate(mut call: impl FnMut(&mut TestCounter)) {
    let mut counter = TestCounter { input: 7 };
    for expected in [8, 9] {
        call(&mut counter);
        assert_eq!(counter.input, expected);
    }
}
