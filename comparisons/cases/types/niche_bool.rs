// Rust leaves this option’s layout unspecified.



use co3::convert::{lang};

include!("../support/mod.rs");

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle, stabby::stabby)]
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

#[cfg(feature = "abi_stable")]
abi_stable_signature!(advance: (Option<bool>) -> Option<bool>);

#[cfg(feature = "co3")]
mod caller {
    co3::ffi! {
        #![unsafe(extern("C"))]
        pub fn advance(input: u8) -> u8;
    }
}

fn main() {
    #[cfg(feature = "co3")]
    {
        assert_eq!(core::mem::size_of_val(&unsafe { co3::encode::<_, lang::C>(None::<bool>) }), 1);
        // CO3 assigns 2 to None, 0 to Some(false), and 1 to Some(true).
        for (input, expected) in [(2, 0), (0, 1), (1, 2)] {
            assert_eq!(caller::advance(input), expected);
        }
    }
    #[cfg(feature = "diplomat")]
    {
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
    #[cfg(any(feature = "uniffi", feature = "boltffi"))]
    {
        // These callers serialize the presence tag and, for Some, a boolean byte.
        let none: &[u8] = &[0];
        let some_false: &[u8] = &[1, 0];
        let some_true: &[u8] = &[1, 1];
        for (input, expected) in [
            (none, some_false),
            (some_false, some_true),
            (some_true, none),
        ] {
            #[cfg(feature = "uniffi")]
            {
                let mut status = uniffi::RustCallStatus::default();
                let buffer = uniffi::RustBuffer::from_vec(input.to_vec());
                let output = uniffi_co3_framework_comparisons_fn_func_advance(buffer, &mut status);
                assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
                assert_eq!(output.destroy_into_vec(), expected);
            }
            #[cfg(feature = "boltffi")]
            {
                let output =
                    unsafe { boltffi_function_10niche_bool7advance(input.as_ptr(), input.len()) };
                assert_eq!(unsafe { output.as_byte_slice() }, expected);
            }
        }
    }
}
