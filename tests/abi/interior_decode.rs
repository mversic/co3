use core::cell::Cell;

use co3::lang;

#[test]
fn shared_interior_decode_preserves_identity_and_mutations() {
    let value = Cell::new(7_u8);
    let encoded = unsafe { co3::encode::<_, lang::C>(&value) };
    let decoded = unsafe { co3::decode::<&Cell<u8>, lang::C>(encoded) }.unwrap();
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
    let encoded = unsafe { co3::encode::<_, lang::C>(&values[..]) };
    let decoded = unsafe { co3::decode::<&[Cell<u8>], lang::C>(encoded) }.unwrap();
    assert_eq!(decoded.as_ptr(), values.as_ptr());
    assert_eq!(decoded.len(), 2);
    decoded[1].set(8);
    assert_eq!(values[1].get(), 8);
    let unchecked = unsafe { co3::decode_unchecked::<&[Cell<u8>]>(encoded) };
    assert_eq!(unchecked.as_ptr(), values.as_ptr());
    let empty = unsafe { co3::encode::<_, lang::C>(&values[..0]) };
    assert!(
        unsafe { co3::decode::<&[Cell<u8>], lang::C>(empty) }
            .unwrap()
            .is_empty()
    );
}
