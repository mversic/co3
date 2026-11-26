// Rust guarantees the nullable one-byte representation of this option.

include!("../support/mod.rs");

use core::num::NonZeroU8;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn advance(input: Option<NonZeroU8>) -> Option<NonZeroU8> {
    NonZeroU8::new(input.map_or(1, |value| value.get().wrapping_add(1)))
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    use core::num::NonZeroU8;

    pub fn advance(input: Option<NonZeroU8>) -> Option<NonZeroU8> {
        super::advance(input)
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn advance(input: Option<NonZeroU8>) -> Option<NonZeroU8>;
}

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]
        #[symbol_name = "niche_nonzero__advance"]
        fn imported_advance(input: u8) -> u8;
    }

    validate(imported_advance);
}

#[cfg(any(
    feature = "uniffi",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi"
))]
fn main() {}

#[cfg(feature = "diplomat")]
fn main() {
    use diplomat_runtime::DiplomatOption;

    unsafe extern "C" {
        fn diplomat_external_advance(input: DiplomatOption<NonZeroU8>)
            -> DiplomatOption<NonZeroU8>;
    }
    assert_eq!(core::mem::size_of::<DiplomatOption<NonZeroU8>>(), 2);
    validate(|byte| {
        let output = unsafe { diplomat_external_advance(NonZeroU8::new(byte).into()) };
        Option::from(output).map_or(0, NonZeroU8::get)
    });
}

fn validate(mut call: impl FnMut(u8) -> u8) {
    for byte in 0..=u8::MAX {
        assert_eq!(call(byte), byte.wrapping_add(1));
    }
}
