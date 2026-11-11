use std::cell::RefCell;

use co3::{ReprC, ffi, raw, rust_spec::RustSpec, tuple::Tuple3};

thread_local! {
    static EVENTS: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn validate(value: &u8) -> bool {
    EVENTS.with(|events| events.borrow_mut().push(*value));
    true
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[rust_spec(non_robust)]
#[repr_c(is_valid = |value| validate(value))]
struct Probe(u8);

#[derive(RustSpec, ReprC)]
struct Record(Probe, Probe, [u8; 1]);

fn tuple(values: (Probe, Probe, Probe)) -> u8 {
    values.0.0 + values.1.0 + values.2.0
}

fn record(value: Record) -> u8 {
    value.0.0 + value.1.0 + value.2[0]
}

raw! {
    fn tuple(values: (Probe, Probe, Probe)) -> u8;
    fn record(value: Record) -> u8;
}

#[unsafe(export_name = "field_order_tuple")]
extern "C" fn receive_tuple(values: Tuple3<CProbe, CProbe, CProbe>) -> u8 {
    values.0.0 + values.1.0 + values.2.0
}

#[unsafe(export_name = "field_order_record")]
extern "C" fn receive_record(value: CRecord) -> u8 {
    value.0.0 + value.1.0 + value.2[0]
}

mod imported {
    use super::*;

    ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "field_order_tuple"]
        pub fn tuple(values: (Probe, Probe, Probe)) -> u8;

        #[symbol_name = "field_order_record"]
        pub fn record(value: Record) -> u8;
    }
}

fn assert_events(expected: &[u8]) {
    EVENTS.with(|events| {
        assert_eq!(&*events.borrow(), expected);
        events.borrow_mut().clear();
    });
}

#[test]
#[cfg(debug_assertions)]
fn imported_arguments_validate_fields_in_declaration_order() {
    assert_eq!(imported::tuple((Probe(1), Probe(2), Probe(3))), 6);
    assert_events(&[1, 2, 3]);

    assert_eq!(imported::record(Record(Probe(1), Probe(2), [3])), 6);
    assert_events(&[1, 2]);
}

#[test]
fn raw_companions_validate_fields_in_declaration_order() {
    assert_eq!(
        unsafe { tuple_raw(Tuple3(CProbe(1), CProbe(2), CProbe(3))) },
        6
    );
    assert_events(&[1, 2, 3]);

    assert_eq!(unsafe { record_raw(CRecord(CProbe(1), CProbe(2), [3])) }, 6);
    assert_events(&[1, 2]);
}
