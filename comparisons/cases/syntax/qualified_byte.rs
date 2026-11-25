// A qualified primitive path misses the mutable byte-slice special case.
include!("../support/mod.rs");

use core::primitive;

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(buf: &mut [primitive::u8]) {
    buf.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    use core::primitive;

    pub fn fill(buf: &mut [primitive::u8]) {
        buf.fill(42);
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(buf: &mut [primitive::u8]);
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(fill: (&mut [primitive::u8]) -> ());

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
