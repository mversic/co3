use core::cell::Cell;

#[test]
fn shared_interior_decode_preserves_identity_and_mutations() {
    let value = Cell::new(7_u8);
    let encoded = co3::encode(&value);
    let decoded = unsafe { co3::decode::<&Cell<u8>>(encoded) }.unwrap();
    assert!(core::ptr::eq(decoded, &value));
    decoded.set(9);
    assert_eq!(value.get(), 9);
    value.set(11);
    assert_eq!(decoded.get(), 11);
    let unchecked = unsafe { co3::decode_unchecked::<&Cell<u8>>(encoded) };
    assert!(core::ptr::eq(unchecked, &value));
}

#[test]
fn shared_interior_slice_decode_preserves_identity_and_mutations() {
    let values = [Cell::new(3_u8), Cell::new(5)];
    let encoded = co3::encode(&values[..]);
    let decoded = unsafe { co3::decode::<&[Cell<u8>]>(encoded) }.unwrap();
    assert_eq!(decoded.as_ptr(), values.as_ptr());
    assert_eq!(decoded.len(), 2);
    decoded[1].set(8);
    assert_eq!(values[1].get(), 8);
    let unchecked = unsafe { co3::decode_unchecked::<&[Cell<u8>]>(encoded) };
    assert_eq!(unchecked.as_ptr(), values.as_ptr());
    let empty = co3::encode(&values[..0]);
    assert!(
        unsafe { co3::decode::<&[Cell<u8>]>(empty) }
            .unwrap()
            .is_empty()
    );
}

#[test]
fn shared_interior_decode_rejects_null() {
    assert!(unsafe { co3::decode::<&Cell<u8>>(co3::reference::CRefMut::null()) }.is_none());
    assert!(unsafe { co3::decode::<&[Cell<u8>]>(co3::slice::CWideRefMut::null()) }.is_none());
}
