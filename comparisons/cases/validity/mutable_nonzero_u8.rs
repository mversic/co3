// Set a nonzero byte through an exclusive reference.
include!("../support/mod.rs");

use core::num::NonZeroU8;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn update(input: &mut NonZeroU8) {
    *input = NonZeroU8::MIN;
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    use core::num::NonZeroU8;

    pub fn update(input: &mut NonZeroU8) {
        *input = NonZeroU8::MIN;
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn update(#[soft] input: &mut NonZeroU8);
}

// Exercise the other call direction with a byte-addressable ABI temporary.
#[cfg(feature = "co3")]
#[unsafe(export_name = "comparison_write_nonzero")]
unsafe extern "C" fn write_byte(output: *mut u8, byte: u8) -> u8 {
    unsafe { output.write(byte) };
    0
}

#[cfg(feature = "co3")]
fn check_writeback(mut call: impl FnMut(&mut NonZeroU8, u8) -> u8) {
    for (byte, expected, final_value) in [(7u8, Some(0u8), 7), (0, None, 1)] {
        let mut value = NonZeroU8::MIN;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| call(&mut value, byte)));
        assert_eq!(result.ok(), expected);
        assert_eq!(value.get(), final_value);
    }
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "mutable_nonzero_u8__update"]
        fn imported_update(input: &mut u8);

        #[symbol_name = "comparison_write_nonzero"]
        fn write_nonzero(#[soft] output: &mut NonZeroU8, byte: u8) -> u8;
    }

    validate(imported_update);
    check_writeback(write_nonzero);
}

#[cfg(any(feature = "uniffi", feature = "safer_ffi", feature = "boltffi"))]
fn main() {}

#[cfg(feature = "diplomat")]
fn main() {
    unsafe extern "C" {
        #[link_name = "diplomat_external_update"]
        fn exported_update(input: *mut u8);
    }
    validate(|byte| unsafe { exported_update(byte) });
}

#[cfg(feature = "interoptopus")]
fn main() {
    unsafe extern "C" {
        #[link_name = "update"]
        fn exported_update(input: *mut u8);
    }
    validate(|byte| unsafe { exported_update(byte) });
}

fn validate(call: impl FnOnce(&mut u8)) {
    let mut byte = 7u8;
    // The caller supplies exclusively borrowed storage containing a valid NonZeroU8.
    call(&mut byte);
    assert_eq!(byte, 1);
}
