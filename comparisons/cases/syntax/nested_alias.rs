// Aliases nested inside Option and a reference.
include!("../support/mod.rs");

pub type Word = u32;
pub type Maybe<T> = Option<T>;

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn read_optional(input: Maybe<&Word>) -> u32 {
    input.copied().unwrap_or(0)
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub type Word = u32;
    pub type Maybe<T> = Option<T>;

    pub fn read_optional(input: Maybe<&Word>) -> u32 {
        input.copied().unwrap_or(0)
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn read_optional(input: Maybe<&Word>) -> u32;
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(read_optional: (Maybe<&Word>) -> u32);

fn main() {}
