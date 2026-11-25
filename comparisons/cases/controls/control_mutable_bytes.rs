// Positive control: literal mutable byte slices work in both backends.
include!("../support/mod.rs");

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
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

#[cfg(feature = "abi_stable")]
abi_stable_signature!(fill: (&mut [u8]) -> ());

#[cfg(feature = "co3")]
#[path = "../support/byte_fill.rs"]
mod caller;

#[cfg(feature = "co3")]
fn main() {
    let mut bytes = [1u8, 2, 3];

    caller::fill(&mut bytes);

    assert_eq!(bytes, [42; 3]);
}

#[cfg(feature = "uniffi")]
fn main() {
    let mut bytes = [1u8, 2, 3];

    {
        let mut status = uniffi::RustCallStatus::default();
        // The synchronous call borrows the live array exclusively.
        let buf = unsafe { uniffi::ForeignBytesMut::from_raw_parts(bytes.as_mut_ptr(), 3) };
        uniffi_co3_framework_comparisons_fn_func_fill(buf, &mut status);
        assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
    }

    assert_eq!(bytes, [42; 3]);
}

#[cfg(feature = "diplomat")]
fn main() {
    let mut bytes = [1u8, 2, 3];

    {
        unsafe extern "C" {
            fn diplomat_external_fill(buf: diplomat_runtime::DiplomatSliceMut<'_, u8>);
        }
        unsafe { diplomat_external_fill(bytes.as_mut_slice().into()) };
    }

    assert_eq!(bytes, [42; 3]);
}

#[cfg(any(
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "abi_stable",
    feature = "stabby"
))]
fn main() {
    let mut bytes = [1u8, 2, 3];

    assert_eq!(bytes, [42; 3]);
}

#[cfg(feature = "boltffi")]
fn main() {
    let mut bytes = [1u8, 2, 3];

    {
        let status = unsafe {
            boltffi_function_21control_mutable_bytes4fill(bytes.as_mut_ptr(), bytes.len())
        };
        assert!(status.is_ok());
    }
    assert_eq!(bytes, [42; 3]);
}
