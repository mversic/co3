// Read private Rust state through a shared borrow of a named opaque type.
include!("../support/mod.rs");

#[cfg(not(feature = "diplomat"))]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec))]
#[cfg_attr(feature = "co3", rust_spec(opaque))]
#[cfg_attr(feature = "uniffi", derive(uniffi::Object))]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi(opaque))]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC, repr(opaque))]
pub struct Counter {
    values: Vec<u32>,
}

#[cfg(not(feature = "diplomat"))]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn counter_value(counter: &Counter) -> u32 {
    counter.values.iter().sum()
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    #[diplomat::opaque]
    pub struct Counter {
        pub(super) values: Vec<u32>,
    }

    pub fn counter_value(counter: &Counter) -> u32 {
        counter.values.iter().sum()
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    type Counter;
    fn counter_value(counter: &Counter) -> u32;
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "opaque_type__counter_value"]
        fn imported_value(counter: &Counter) -> u32;
    }

    validate(|counter| imported_value(&counter));
}

#[cfg(feature = "uniffi")]
fn main() {
    validate(|counter| {
        use std::sync::Arc;
        let handle = <Arc<Counter> as uniffi::Lower<UniFfiTag>>::lower(Arc::new(counter));
        let mut status = uniffi::RustCallStatus::default();
        let value = uniffi_co3_framework_comparisons_fn_func_counter_value(handle, &mut status);
        assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
        value
    });
}

#[cfg(feature = "diplomat")]
fn main() {
    use diplomat_case::Counter;
    validate(|counter| {
        unsafe extern "C" {
            #[link_name = "diplomat_external_counter_value"]
            fn exported_value(handle: *const core::ffi::c_void) -> u32;
        }
        let handle = core::ptr::from_ref(&counter).cast();
        unsafe { exported_value(handle) }
    });
}

#[cfg(any(feature = "interoptopus", feature = "safer_ffi"))]
fn main() {
    validate(|counter| {
        unsafe extern "C" {
            #[link_name = "counter_value"]
            fn exported_value(handle: *const core::ffi::c_void) -> u32;
        }
        let handle = core::ptr::from_ref(&counter).cast();
        unsafe { exported_value(handle) }
    });
}

#[cfg(feature = "boltffi")]
fn main() {
    validate(|counter| {
        let handle = __BoltffiCounterHandle::new(counter) as u64;
        let value = unsafe { boltffi_function_11opaque_type13counter_value(handle) };
        unsafe { boltffi_release_class_11opaque_type7Counter(handle) };
        value
    });
}

#[cfg(feature = "diplomat")]
type TestCounter = diplomat_case::Counter;
#[cfg(not(feature = "diplomat"))]
type TestCounter = Counter;

fn validate(mut call: impl FnMut(TestCounter) -> u32) {
    for (values, expected) in [(vec![], 0), (vec![12, 30], 42)] {
        assert_eq!(call(TestCounter { values }), expected);
    }
}
