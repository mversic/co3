// Same mutable slice as control_mutable_bytes, with the element spelling from type_macro.
include!("../support/mod.rs");

macro_rules! byte {
    () => {
        u8
    };
}

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(buf: &mut [byte!()]) {
    buf.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {

    pub fn fill(buf: &mut [byte!()]) {
        buf.fill(42);
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(buf: &mut [byte!()]);
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(fill: (&mut [byte!()]) -> ());

#[cfg(feature = "co3")]
#[path = "../support/byte_fill.rs"]
mod caller;

fn main() {
    #[cfg(feature = "co3")]
    caller::check([42; 3]);
}
