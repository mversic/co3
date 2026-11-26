// An opaque Counter exposes increment(&mut self) and keeps its fields private.
include!("../support/mod.rs");

// BoltFFI requires the caller to guarantee single-threaded access.
#[cfg(not(any(feature = "diplomat", feature = "safer_ffi")))]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
#[cfg_attr(feature = "boltffi", boltffi::export(single_threaded))]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi(service))]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec))]
#[cfg_attr(feature = "co3", rust_spec(opaque))]
pub struct Counter {
    input: u32,
}

#[cfg(not(any(feature = "diplomat", feature = "safer_ffi")))]
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
    #[diplomat::opaque_mut]
    pub struct Counter {
        pub(super) input: u32,
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

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "opaque_mutable_receiver__Counter__increment"]
        fn imported_increment(counter: &mut Counter);
    }

    validate(|mut counter| {
        imported_increment(&mut counter);
        counter
    });
}

#[cfg(any(feature = "uniffi", feature = "safer_ffi"))]
fn main() {}

#[cfg(feature = "diplomat")]
fn main() {
    unsafe extern "C" {
        #[link_name = "Counter_increment"]
        fn exported_increment(handle: *mut core::ffi::c_void);
    }
    validate(|mut counter| {
        unsafe { exported_increment(core::ptr::from_mut(&mut counter).cast()) };

        counter
    });
}

#[cfg(feature = "interoptopus")]
fn main() {
    validate(|mut counter| {
        unsafe { counter_increment(&mut counter) };
        counter
    });
}

#[cfg(feature = "boltffi")]
fn main() {
    validate(|mut counter| {
        let handle = __BoltffiCounterHandle::new(counter);
        let status = unsafe {
            boltffi_method_class_23opaque_mutable_receiver7Counter9increment(handle as u64)
        };
        assert!(status.is_ok());
        let counter = unsafe { __BoltffiCounterHandle::take(handle) }.unwrap();
        counter
    });
}

#[cfg(feature = "diplomat")]
type TestCounter = diplomat_case::Counter;
#[cfg(not(any(feature = "diplomat", feature = "safer_ffi")))]
type TestCounter = Counter;

#[cfg(not(feature = "safer_ffi"))]
fn validate(call: impl FnOnce(TestCounter) -> TestCounter) {
    let counter = call(TestCounter { input: 7 });
    assert_eq!(counter.input, 8);
}
