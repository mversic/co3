// Rust guarantees the nullable one-byte representation of this option.

use co3::convert::lang;

include!("../support/mod.rs");

use core::num::NonZeroU8;

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle, stabby::stabby)]
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

#[cfg(feature = "abi_stable")]
abi_stable_signature!(advance: (Option<NonZeroU8>) -> Option<NonZeroU8>);

#[cfg(feature = "co3")]
mod caller {
    co3::ffi! {
        #![unsafe(extern("C"))]
        pub fn advance(input: u8) -> u8;
    }
}

#[cfg(feature = "co3")]
fn main() {
    assert_eq!(
        core::mem::size_of_val(&unsafe { co3::encode::<_, lang::C>(None::<NonZeroU8>) }),
        1
    );
    for byte in 0..=u8::MAX {
        let expected = byte.wrapping_add(1);

        assert_eq!(caller::advance(byte), expected);
    }
}

#[cfg(any(
    feature = "uniffi",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi"
))]
fn main() {
    for byte in 0..=u8::MAX {
        let expected = byte.wrapping_add(1);
    }
}

#[cfg(feature = "diplomat")]
fn main() {
    for byte in 0..=u8::MAX {
        let expected = byte.wrapping_add(1);

        {
            use diplomat_runtime::DiplomatOption;
            unsafe extern "C" {
                fn diplomat_external_advance(
                    input: DiplomatOption<NonZeroU8>,
                ) -> DiplomatOption<NonZeroU8>;
            }
            assert_eq!(core::mem::size_of::<DiplomatOption<NonZeroU8>>(), 2);
            let output = unsafe { diplomat_external_advance(NonZeroU8::new(byte).into()) };
            assert_eq!(Option::from(output), NonZeroU8::new(expected));
        }
    }
}

#[cfg(any(feature = "abi_stable", feature = "stabby"))]
fn main() {
    for byte in 0..=u8::MAX {
        let expected = byte.wrapping_add(1);

        {
            let export: extern "C" fn(Option<NonZeroU8>) -> Option<NonZeroU8> = advance;
            assert_eq!(core::mem::size_of::<Option<NonZeroU8>>(), 1);
            assert_eq!(export(NonZeroU8::new(byte)), NonZeroU8::new(expected));
        }
    }
}
