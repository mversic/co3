// Framework-provided stable slices: compare this control with its alias case.
include!("../support/mod.rs");

#[cfg(feature = "abi_stable")]
use abi_stable::std_types::RSliceMut as StableSlice;
#[cfg(feature = "stabby")]
use stabby::slice::SliceMut as StableSlice;

pub type BytesMut<'a> = StableSlice<'a, u8>;

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
pub fn fill(mut buf: BytesMut<'_>) {
    buf.fill(42);
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(fill: (BytesMut<'_>) -> ());

fn main() {}
