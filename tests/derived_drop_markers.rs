use co3::{ReprC, rust_spec::RustSpec};

#[derive(Debug, PartialEq, Eq, RustSpec, ReprC)]
#[repr(C)]
struct Record<T>(T);

#[test]
fn no_drop_derived_type_converts_through_public_functions() {
    assert_eq!(
        unsafe { co3::decode::<Record<u32>>(co3::encode(Record(7))) },
        Some(Record(7))
    );
}

#[test]
fn auto_drop_derived_type_converts_through_public_functions() {
    let encoded = co3::encode(Record(Box::new(9_u32)));
    let decoded: Record<Box<u32>> = unsafe { co3::decode(encoded) }.unwrap();
    assert_eq!(*decoded.0, 9);
}
