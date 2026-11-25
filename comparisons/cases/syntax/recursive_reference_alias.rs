
include!("../support/mod.rs");

pub type LinkTarget = Node;

#[repr(C)]
#[cfg_attr(feature = "abi_stable", derive(abi_stable::StableAbi))]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
pub struct Node {
    pub next: &'static LinkTarget,
}

fn main() {}
