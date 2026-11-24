// A named unit return type.
include!("../support/mod.rs");

pub type Done = ();

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn finish(input: u32) -> Done {
    let _ = input;
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub type Done = ();

    pub fn finish(input: u32) -> Done {
        let _ = input;
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn finish(input: u32) -> Done;
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(finish: (u32) -> Done);

fn main() {}
