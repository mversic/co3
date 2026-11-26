// Rust leaves this option’s layout unspecified.

include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn advance(input: Option<bool>) -> Option<bool> {
    match input {
        None => Some(false),
        Some(false) => Some(true),
        Some(true) => None,
    }
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub fn advance(input: Option<bool>) -> Option<bool> {
        super::advance(input)
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn advance(input: Option<bool>) -> Option<bool>;
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]
        #[symbol_name = "niche_bool__advance"]
        fn imported_advance(input: u8) -> u8;
    }

    // CO3 assigns 2 to None, 0 to Some(false), and 1 to Some(true).
    for (input, expected) in [(2, 0), (0, 1), (1, 2)] {
        assert_eq!(imported_advance(input), expected);
    }
}

#[cfg(feature = "uniffi")]
fn main() {
    validate_wire(|input| {
        let mut status = uniffi::RustCallStatus::default();
        let buffer = uniffi::RustBuffer::from_vec(input.to_vec());
        let output = uniffi_co3_framework_comparisons_fn_func_advance(buffer, &mut status);
        assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
        output.destroy_into_vec()
    });
}

#[cfg(feature = "diplomat")]
fn main() {
    use diplomat_runtime::DiplomatOption;
    unsafe extern "C" {
        fn diplomat_external_advance(input: DiplomatOption<bool>) -> DiplomatOption<bool>;
    }
    assert_eq!(core::mem::size_of::<DiplomatOption<bool>>(), 2);
    for input in [None, Some(false), Some(true)] {
        let output = unsafe { diplomat_external_advance(input.into()) };
        assert_eq!(Option::from(output), advance(input));
    }
}

#[cfg(any(feature = "interoptopus", feature = "safer_ffi"))]
fn main() {}

#[cfg(feature = "boltffi")]
fn main() {
    validate_wire(|input| {
        let output = unsafe { boltffi_function_10niche_bool7advance(input.as_ptr(), input.len()) };
        unsafe { output.as_byte_slice() }.to_vec()
    });
}

fn validate_wire(mut call: impl FnMut(&[u8]) -> Vec<u8>) {
    // A serialized presence tag followed by a boolean byte for Some.
    let none: &[u8] = &[0];
    let some_false: &[u8] = &[1, 0];
    let some_true: &[u8] = &[1, 1];
    for (input, expected) in [
        (none, some_false),
        (some_false, some_true),
        (some_true, none),
    ] {
        assert_eq!(call(input), expected);
    }
}
