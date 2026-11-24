


use co3::convert::{lang};

include!("../support/mod.rs");

#[cfg_attr(feature = "abi_stable", derive(abi_stable::StableAbi))]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "boltffi", boltffi::data)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[repr(C)]
pub struct Node {
    pub weight: u32,
    pub children: Vec<u32>,
}

#[cfg_attr(feature = "abi_stable", abi_stable::sabi_extern_fn)]
#[cfg_attr(feature = "stabby", no_mangle)]
#[cfg_attr(feature = "stabby", stabby::stabby)]
#[cfg_attr(feature = "uniffi", uniffi::export)]
#[cfg_attr(feature = "boltffi", boltffi::export)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::ffi_export)]
pub fn echo(node: Node) -> Node {
    node
}

#[cfg(feature = "diplomat")]
#[diplomat::bridge]
pub mod diplomat_case {
use co3::convert::{lang};




    #[repr(C)]
    pub struct Node {
        pub weight: u32,
        pub children: Vec<u32>,
    }

    pub fn echo(node: Node) -> Node {
        node
    }
}

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(export("C"))]
    fn echo(node: move Node) -> move Node;
}

#[cfg(feature = "abi_stable")]
abi_stable_signature!(echo: (Node) -> Node);

#[cfg(feature = "boltffi")]
use boltffi_function_20control_vector_field4echo as exported_echo;

#[cfg(any(feature = "co3", feature = "uniffi", feature = "boltffi"))]
#[path = "../support/owned_echo.rs"]
mod caller;

fn main() {
    #[cfg(any(feature = "co3", feature = "uniffi", feature = "boltffi"))]
    {
        let result = caller::echo(Node { weight: 7, children: vec![19, 23] });
        assert_eq!(result.weight, 7);
        assert_eq!(result.children, [19, 23]);
    }
}
