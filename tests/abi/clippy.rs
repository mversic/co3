use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct ManyFields {
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    g: u8,
    h: u8,
    i: u8,
    j: u8,
    k: u8,
}

#[test]
fn generated_many_field_constructor() {
    let fields = unsafe { CManyFields::from_fields(1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11) };
    assert_eq!(fields.into_fields(), (1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11));
}
