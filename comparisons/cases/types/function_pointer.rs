// An actual C function pointer rather than a named callback interface.
include!("../support/mod.rs");

pub type Callback = extern "C" fn(u32) -> u32;

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn apply(callback: Callback, input: u32) -> u32 {
    callback(input)
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub type Callback = extern "C" fn(u32) -> u32;

    pub fn apply(callback: Callback, input: u32) -> u32 {
        callback(input)
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn apply(callback: Callback, input: u32) -> u32;
}

extern "C" fn increment(input: u32) -> u32 {
    input + 1
}

#[cfg(feature = "co3")]
fn main() {
    {
        unsafe extern "C" {
            #[link_name = "function_pointer__apply"]
            fn exported_apply(callback: Callback, input: u32) -> u32;
        }
        assert_eq!(unsafe { exported_apply(increment, 41) }, 42);
    }
}

#[cfg(any(
    feature = "uniffi",
    feature = "boltffi",
    feature = "abi_stable",
    feature = "stabby"
))]
fn main() {}

#[cfg(feature = "diplomat")]
fn main() {
    {
        unsafe extern "C" {
            #[link_name = "diplomat_external_apply"]
            fn exported_apply(callback: Callback, input: u32) -> u32;
        }
        assert_eq!(unsafe { exported_apply(increment, 41) }, 42);
    }
}

#[cfg(any(feature = "interoptopus", feature = "safer_ffi"))]
fn main() {
    {
        unsafe extern "C" {
            #[link_name = "apply"]
            fn exported_apply(callback: Callback, input: u32) -> u32;
        }
        assert_eq!(unsafe { exported_apply(increment, 41) }, 42);
    }
}
