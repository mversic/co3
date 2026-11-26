// General mutable scalar borrowing.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn increment(input: &mut u32) {
    *input += 1;
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn increment(input: &mut u32) {
        *input += 1;
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn increment(input: &mut u32);
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "mutable_scalar__increment"]
        fn imported_increment(value: &mut u32);
    }

    validate(imported_increment);
}

#[cfg(feature = "uniffi")]
fn main() {}

#[cfg(feature = "diplomat")]
fn main() {
    unsafe extern "C" {
        #[link_name = "diplomat_external_increment"]
        fn exported_increment(value: &mut u32);
    }
    validate(|value| {
        unsafe { exported_increment(value) };
    });
}

#[cfg(any(feature = "interoptopus", feature = "safer_ffi"))]
fn main() {
    unsafe extern "C" {
        #[link_name = "increment"]
        fn exported_increment(value: &mut u32);
    }
    validate(|value| {
        unsafe { exported_increment(value) };
    });
}

#[cfg(feature = "boltffi")]
fn main() {
    // This entry point takes the integer by value and returns only the status.
    let status = unsafe { boltffi_function_14mutable_scalar9increment(7) };
    assert!(status.is_ok());
}

fn validate(call: impl FnOnce(&mut u32)) {
    let mut value = 7;
    call(&mut value);
    assert_eq!(value, 8);
}
