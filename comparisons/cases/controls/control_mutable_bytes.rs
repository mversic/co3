// Positive control: literal mutable byte slices work in both backends.
include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(buf: &mut [u8]) {
    buf.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn fill(buf: &mut [u8]) {
        buf.fill(42);
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(buf: &mut [u8]);
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "control_mutable_bytes__fill"]
        fn exported_fill(buf: &mut [u8]);
    }

    validate(exported_fill);
}

#[cfg(feature = "uniffi")]
fn main() {
    validate(|bytes| {
        let mut status = uniffi::RustCallStatus::default();
        // The synchronous call borrows the live array exclusively.
        let buf = unsafe { uniffi::ForeignBytesMut::from_raw_parts(bytes.as_mut_ptr(), 3) };
        uniffi_co3_framework_comparisons_fn_func_fill(buf, &mut status);
        assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
    });
}

#[cfg(feature = "diplomat")]
fn main() {
    unsafe extern "C" {
        fn diplomat_external_fill(buf: diplomat_runtime::DiplomatSliceMut<'_, u8>);
    }

    validate(|bytes| {
        unsafe { diplomat_external_fill(bytes.into()) };
    });
}

#[cfg(any(feature = "interoptopus", feature = "safer_ffi"))]
fn main() {}

#[cfg(feature = "boltffi")]
fn main() {
    validate(|bytes| {
        let status = unsafe {
            boltffi_function_21control_mutable_bytes4fill(bytes.as_mut_ptr(), bytes.len())
        };
        assert!(status.is_ok());
    });
}

fn validate(call: impl FnOnce(&mut [u8])) {
    let mut bytes = [1u8, 2, 3];
    call(&mut bytes);
    assert_eq!(bytes, [42; 3]);
}
