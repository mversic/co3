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
    {
        let mut counter = Counter { input: 7 };

        {
            unsafe extern "C" {
                #[link_name = "opaque_mutable_receiver__Counter__increment"]
                fn exported_increment(handle: *mut core::ffi::c_void);
            }
            unsafe { exported_increment(core::ptr::from_mut(&mut counter).cast()) };
        }

        assert_eq!(counter.input, 8);
    }
}

#[cfg(any(
    feature = "uniffi",
    feature = "safer_ffi",
    feature = "abi_stable",
    feature = "stabby"
))]
fn main() {}

#[cfg(feature = "diplomat")]
fn main() {
    {
        use diplomat_case::Counter;
        let mut counter = Counter { input: 7 };

        {
            unsafe extern "C" {
                #[link_name = "Counter_increment"]
                fn exported_increment(handle: *mut core::ffi::c_void);
            }
            unsafe { exported_increment(core::ptr::from_mut(&mut counter).cast()) };
        }

        assert_eq!(counter.input, 8);
    }
}

#[cfg(feature = "interoptopus")]
fn main() {
    {
        let mut counter = Counter { input: 7 };

        unsafe { counter_increment(&mut counter) };
        assert_eq!(counter.input, 8);
    }
}

#[cfg(feature = "boltffi")]
fn main() {
    {
        let handle = __BoltffiCounterHandle::new(Counter { input: 7 });
        let status = unsafe {
            boltffi_method_class_23opaque_mutable_receiver7Counter9increment(handle as u64)
        };
        assert!(status.is_ok());
        let counter = unsafe { __BoltffiCounterHandle::take(handle) }.unwrap();
        assert_eq!(counter.input, 8);
    }
}
