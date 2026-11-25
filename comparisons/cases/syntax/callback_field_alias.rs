// Same function-pointer field, written through an alias.

include!("../support/mod.rs");

pub type Callback = extern "C" fn(u32) -> u32;

#[repr(C)]
#[cfg_attr(feature = "abi_stable", derive(abi_stable::StableAbi))]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
pub struct Callbacks {
    pub apply: Callback,
}

extern "C" fn increment(input: u32) -> u32 { input + 1 }

fn main() {
    let callbacks = Callbacks { apply: increment };
    assert_eq!((callbacks.apply)(41), 42);
}
