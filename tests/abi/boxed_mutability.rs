use core::cell::Cell;

use co3::stored::{DecodeOwned, EncodeMove, EncodeMoveUnchecked, Store};

#[test]
fn stable_interior_box_direct_round_trip_preserves_allocation() {
    let value = Box::new(Cell::new(true));
    let original = core::ptr::from_ref(&*value);
    let encoded = unsafe { value.soft_encode_unchecked(&mut ()) };
    assert_eq!(encoded.as_ptr().cast::<Cell<bool>>(), original.cast_mut());
    let decoded = unsafe { Box::<Cell<bool>>::soft_decode(encoded, &mut ()) }.unwrap();
    assert_eq!(core::ptr::from_ref(&*decoded), original);
    assert!(decoded.get());
}

#[test]
fn stable_interior_slice_box_direct_round_trip_preserves_allocation() {
    let value = Box::new([Cell::new(true), Cell::new(false)]) as Box<[Cell<bool>]>;
    let original = value.as_ptr();
    let encoded = unsafe { value.soft_encode_unchecked(&mut ()) };
    assert_eq!(encoded.as_ptr().cast::<Cell<bool>>(), original.cast_mut());
    let decoded = unsafe { Box::<[Cell<bool>]>::soft_decode(encoded, &mut ()) }.unwrap();
    assert_eq!(decoded.as_ptr(), original);
    assert_eq!(
        decoded.iter().map(Cell::get).collect::<Vec<_>>(),
        [true, false]
    );
}

#[test]
fn unstable_interior_box_checked_round_trip() {
    let mut encode_store = Default::default();
    let encoded = Box::new(Cell::new((7_u8,))).soft_encode(&mut encode_store);
    let mut decode_store = Default::default();
    let decoded = unsafe { Box::<Cell<(u8,)>>::soft_decode(encoded, &mut decode_store) }.unwrap();
    assert_eq!(decoded.get(), (7,));
    decode_store.sync().unwrap();
    encode_store.sync().unwrap();
}

#[test]
fn interior_box_validation_checks_null_and_pointee() {
    use co3::{boxed::CBoxCell, transmute::CheckedTransmute};

    assert!(!unsafe { Box::<Cell<bool>>::is_valid(&CBoxCell::null()) });
    let carrier = unsafe { Box::new(Cell::new(true)).soft_encode_unchecked(&mut ()) };
    assert!(unsafe { Box::<Cell<bool>>::is_valid(&carrier) });
    drop(unsafe { Box::<Cell<bool>>::soft_decode(carrier, &mut ()) }.unwrap());
}
