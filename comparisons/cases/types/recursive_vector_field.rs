include!("../support/mod.rs");

#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "boltffi", boltffi::data)]
#[cfg_attr(feature = "interoptopus", interoptopus::ffi)]
#[cfg_attr(feature = "safer_ffi", safer_ffi::derive_ReprC)]
#[cfg_attr(feature = "co3", derive(co3::rust_spec::RustSpec, co3::ReprC))]
#[repr(C)]
pub struct Node {
    pub weight: u32,
    pub children: Vec<Node>,
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
        pub children: Vec<Node>,
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

#[cfg(feature = "boltffi")]
use boltffi_function_22recursive_vector_field4echo as exported_echo;

#[cfg(feature = "co3")]
fn main() {
    co3::ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "recursive_vector_field__echo"]
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

#[cfg(feature = "boltffi")]
fn main() {
    validate(|node| {
        let input = boltffi::__private::FfiBuf::wire_encode(&node);
        let output = unsafe { exported_echo(input.as_ptr(), input.len()) };
        boltffi::__private::wire::decode(unsafe { output.as_byte_slice() }).unwrap()
    });
}

#[cfg(any(feature = "diplomat", feature = "interoptopus", feature = "safer_ffi"))]
fn main() {}

fn validate(call: impl FnOnce(Node) -> Node) {
    let node = Node {
        weight: 7,
        children: vec![Node {
            weight: 19,
            children: vec![],
        }],
    };
    let result = call(node);
    assert_eq!(result.weight, 7);
    assert_eq!(result.children.len(), 1);
    assert_eq!(result.children[0].weight, 19);
    assert!(result.children[0].children.is_empty());
}
