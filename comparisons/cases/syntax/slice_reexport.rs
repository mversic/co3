// Same mutable slice as control_mutable_bytes, with the element spelling from reexport.
include!("../support/mod.rs");

mod model {
    pub use core::primitive::u8 as Byte;
}
use model::Byte;

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(buf: &mut [Byte]) {
    buf.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    mod model {
        pub use core::primitive::u8 as Byte;
    }
    use model::Byte;

    pub fn fill(buf: &mut [Byte]) {
        buf.fill(42);
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(buf: &mut [Byte]);
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(fill: (&mut [Byte]) -> ());

#[cfg(feature = "co3")]
#[path = "../support/byte_fill.rs"]
mod caller;

#[cfg(feature = "co3")]
fn main() {
    caller::check([42; 3]);
}

#[cfg(any(
    feature = "uniffi",
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi",
    feature = "abi_stable",
    feature = "stabby"
))]
fn main() {}
