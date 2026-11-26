include!("../support/mod.rs");

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

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "recursive_optional_box__echo"]
        fn imported_echo(node: move Node) -> move Node;
    }

    validate(imported_echo);
}

#[cfg(feature = "uniffi")]
fn main() {
    validate(|node| {
        let input = <Node as uniffi::Lower<UniFfiTag>>::lower(node);
        let mut status = uniffi::RustCallStatus::default();
        let output = uniffi_co3_framework_comparisons_fn_func_echo(input, &mut status);
        assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
        <Node as uniffi::Lift<UniFfiTag>>::try_lift(output).unwrap()
    });
}

#[cfg(any(
    feature = "diplomat",
    feature = "interoptopus",
    feature = "safer_ffi",
    feature = "boltffi"
))]
fn main() {}

fn validate(call: impl FnOnce(Node) -> Node) {
    let node = Node {
        weight: 7,
        next: Some(Box::new(Node {
            weight: 19,
            next: None,
        })),
    };
    let result = call(node);
    assert_eq!(result.weight, 7);
    let child = result.next.as_ref().unwrap();
    assert_eq!(child.weight, 19);
    assert!(child.next.is_none());
}
