use co3::convert::lang;

include!("../support/mod.rs");

#[cfg(feature = "abi_stable")]
use abi_stable::std_types::RVec as Vec;
#[cfg(feature = "co3")]
use co3::vec::Vec;
#[cfg(feature = "stabby")]
use stabby::vec::Vec;

#[cfg_attr(feature = "abi_stable", derive(abi_stable::StableAbi))]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[repr(C)]
pub struct Node {
    pub weight: u32,
    pub children: Vec<Node>,
}

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
pub fn echo(node: Node) -> Node {
    node
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]
    fn echo(node: move Node) -> move Node;
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(echo: (Node) -> Node);

#[cfg(any(
    feature = "co3",
    feature = "uniffi",
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi"
))]
fn main() {}

#[cfg(any(feature = "abi_stable", feature = "stabby"))]
fn main() {
    {
        let export: extern "C" fn(Node) -> Node = echo;
        let mut children = Vec::new();
        children.push(Node {
            weight: 23,
            children: Vec::new(),
        });
        let result = export(Node {
            weight: 7,
            children,
        });
        assert_eq!(result.weight, 7);
        assert_eq!(result.children.len(), 1);
        assert_eq!(result.children[0].weight, 23);
        assert!(result.children[0].children.is_empty());
    }
}
