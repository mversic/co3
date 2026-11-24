use crate::Node;

#[cfg(feature = "co3")]
co3::ffi! {
    #![unsafe(extern("C"))]
    pub fn echo(node: move Node) -> move Node;
}

#[cfg(feature = "uniffi")]
pub fn echo(node: Node) -> Node {
    let input = <Node as uniffi::Lower<crate::UniFfiTag>>::lower(node);
    let mut status = uniffi::RustCallStatus::default();
    let output = crate::uniffi_co3_framework_comparisons_fn_func_echo(input, &mut status);
    assert_eq!(status.code, uniffi::RustCallStatusCode::Success);
    <Node as uniffi::Lift<crate::UniFfiTag>>::try_lift(output).unwrap()
}

#[cfg(feature = "boltffi")]
pub fn echo(node: Node) -> Node {
    let input = boltffi::__private::FfiBuf::wire_encode(&node);
    // The input buffer is live for the call. The returned wire buffer owns its bytes.
    let output = unsafe { crate::exported_echo(input.as_ptr(), input.len()) };
    boltffi::__private::wire::decode(unsafe { output.as_byte_slice() }).unwrap()
}
