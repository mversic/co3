use co3::convert::lang;

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
    pub next: Option<Box<Node>>,
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
    use co3::convert::lang;

    #[repr(C)]
    pub struct Node {
        pub weight: u32,
        pub next: Option<Box<Node>>,
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

#[cfg(any(feature = "co3", feature = "uniffi"))]
#[path = "../support/owned_echo.rs"]
mod caller;

#[cfg(any(feature = "co3", feature = "uniffi"))]
fn main() {
    {
        let result = caller::echo(Node {
            weight: 7,
            next: Some(Box::new(Node {
                weight: 19,
                next: None,
            })),
        });
        assert_eq!(result.weight, 7);
        let child = result.next.unwrap();
        assert_eq!(child.weight, 19);
        assert!(child.next.is_none());
    }
}

#[cfg(any(
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi",
    feature = "abi_stable",
    feature = "stabby"
))]
fn main() {}
