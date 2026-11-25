// Compare the boolean option layouts provided by the two Rust-to-Rust frameworks.
include!("../support/mod.rs");

#[cfg(feature = "abi_stable")]
use abi_stable::std_types::ROption as FrameworkOption;
#[cfg(feature = "stabby")]
use stabby::abi::option::Option as FrameworkOption;

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle, stabby::stabby)]
pub fn advance(input: FrameworkOption<bool>) -> FrameworkOption<bool> {
    match input.into() {
        None => Some(false),
        Some(false) => Some(true),
        Some(true) => None,
    }
    .into()
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(advance: (FrameworkOption<bool>) -> FrameworkOption<bool>);

#[cfg(any(
    feature = "co3",
    feature = "uniffi",
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi"
))]
fn main() {
    let export: extern "C" fn(FrameworkOption<bool>) -> FrameworkOption<bool> = advance;
    for (input, expected) in [
        (None, Some(false)),
        (Some(false), Some(true)),
        (Some(true), None),
    ] {
        assert_eq!(Option::from(export(input.into())), expected);
    }
}

#[cfg(feature = "abi_stable")]
fn main() {
    assert_eq!(core::mem::size_of::<FrameworkOption<bool>>(), 2);

    let export: extern "C" fn(FrameworkOption<bool>) -> FrameworkOption<bool> = advance;
    for (input, expected) in [
        (None, Some(false)),
        (Some(false), Some(true)),
        (Some(true), None),
    ] {
        assert_eq!(Option::from(export(input.into())), expected);
    }
}

#[cfg(feature = "stabby")]
fn main() {
    assert_eq!(core::mem::size_of::<FrameworkOption<bool>>(), 1);
    let export: extern "C" fn(FrameworkOption<bool>) -> FrameworkOption<bool> = advance;
    for (input, expected) in [
        (None, Some(false)),
        (Some(false), Some(true)),
        (Some(true), None),
    ] {
        assert_eq!(Option::from(export(input.into())), expected);
    }
}
