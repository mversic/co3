// Framework-provided stable slices: compare this control with its alias case.
include!("../support/mod.rs");

#[cfg(feature = "abi_stable")]
use abi_stable::std_types::RSliceMut as StableSlice;
#[cfg(feature = "stabby")]
use stabby::slice::SliceMut as StableSlice;

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
pub fn fill(mut buf: StableSlice<'_, u8>) {
    buf.fill(42);
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(fill: (StableSlice<'_, u8>) -> ());

fn main() {}
