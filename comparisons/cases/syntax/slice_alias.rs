// An alias hides the complete unsized slice type.
// Diplomat BUILD is not ABI parity: the alias wrapper retains &mut Bytes.
// See the expansion audit in README.md.
include!("../support/mod.rs");

pub type Bytes = [u8];

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn fill(buf: &mut Bytes) {
    buf.fill(42);
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
    pub type Bytes = [u8];

    pub fn fill(buf: &mut Bytes) {
        buf.fill(42);
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]

    fn fill(buf: &mut Bytes);
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(fill: (&mut Bytes) -> ());

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
