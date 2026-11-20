// A record containing a borrow and a lifetime parameter.
include!("../support/mod.rs");

#[derive(Clone, Copy)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "boltffi", boltffi::data)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[repr(C)]
pub struct View<'a> {
    pub input: &'a u32,
}

#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn read(view: View<'_>) -> u32 {
    *view.input
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    #[derive(Clone, Copy)]
    #[repr(C)]
    pub struct View<'a> {
        pub input: &'a u32,
    }

    pub fn read(view: View<'_>) -> u32 {
        *view.input
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn read(view: View<'_>) -> u32;
}

fn main() {}
