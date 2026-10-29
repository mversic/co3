use co3::{ReprC, borrow::Borrow, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(u8)]
enum Tagged {
    Value(Box<u32>) = 1,
    Empty,
}

#[derive(RustSpec, ReprC)]
#[repr(C, u8)]
enum TaggedPayload {
    Value(Box<u32>) = 1,
    Empty,
}

#[test]
fn borrowed_enum_companions_do_not_reborrow_fields() {
    let value = Tagged::Value(Box::new(7));
    let view = value.borrow();
    assert!(matches!(Tagged::from_borrow(view), Tagged::Value(value) if *value == 7));
    let value = TaggedPayload::Value(Box::new(9));
    let view = value.borrow();
    assert!(matches!(TaggedPayload::from_borrow(view), TaggedPayload::Value(value) if *value == 9));
}
