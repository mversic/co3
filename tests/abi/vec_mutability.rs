use core::cell::Cell;

use co3::stored::{DecodeOwned, DirectEncodeOwned, EncodeOwned, Store};

#[test]
fn stable_interior_vec_direct_encoding_transfers_allocation() {
    let values = Box::<[Cell<bool>]>::from([Cell::new(true), Cell::new(false)]).into_vec();
    let original = values.as_ptr();
    let encoded = unsafe { values.soft_encode_unchecked(&mut ()) };
    assert_eq!(encoded.as_ptr().cast::<Cell<bool>>(), original.cast_mut());
    let mut store = Default::default();
    let decoded = unsafe { Vec::<Cell<bool>>::soft_decode(encoded, &mut store) }.unwrap();
    assert_eq!(decoded.as_ptr(), original);
    assert_eq!(
        decoded.iter().map(Cell::get).collect::<Vec<_>>(),
        [true, false]
    );
    store.sync().unwrap();
}

#[test]
fn unstable_interior_vec_checked_round_trip() {
    let mut encode_store = Default::default();
    let encoded = vec![Cell::new((3_u8,)), Cell::new((7,))].soft_encode(&mut encode_store);
    let mut decode_store = Default::default();
    let decoded = unsafe { Vec::<Cell<(u8,)>>::soft_decode(encoded, &mut decode_store) }.unwrap();
    assert_eq!(
        decoded.iter().map(Cell::get).collect::<Vec<_>>(),
        [(3,), (7,)]
    );
    decode_store.sync().unwrap();
    encode_store.sync().unwrap();
}

#[test]
fn unstable_interior_vec_unchecked_round_trip() {
    let mut encode_store = Default::default();
    let encoded = unsafe { vec![Cell::new((9_u8,))].soft_encode_unchecked(&mut encode_store) };
    let mut decode_store = Default::default();
    let decoded = unsafe { Vec::<Cell<(u8,)>>::soft_decode_unchecked(encoded, &mut decode_store) };
    assert_eq!(decoded[0].get(), (9,));
    decode_store.sync().unwrap();
    encode_store.sync().unwrap();
}

#[test]
fn robust_vec_checked_encoding_transfers_allocation() {
    let values = Box::<[u32]>::from([3, 7]).into_vec();
    let original = values.as_ptr();
    let encoded = values.soft_encode(&mut ());
    assert_eq!(encoded.as_ptr(), original.cast_mut());
    assert_eq!(&*unsafe { encoded.into_box() }.unwrap(), &[3, 7]);

    let values = Box::<[Cell<u8>]>::from([Cell::new(3), Cell::new(7)]).into_vec();
    let original = values.as_ptr();
    let encoded = values.soft_encode(&mut ());
    assert_eq!(encoded.as_ptr().cast::<Cell<u8>>(), original.cast_mut());
    let mut store = Default::default();
    let decoded = unsafe { Vec::<Cell<u8>>::soft_decode(encoded, &mut store) }.unwrap();
    assert_eq!(decoded.iter().map(Cell::get).collect::<Vec<_>>(), [3, 7]);
    store.sync().unwrap();
}

#[test]
fn nonrobust_vec_keeps_element_writeback_stores() {
    let mut values = [false, false];
    let mut store = Default::default();
    let encoded = values
        .iter_mut()
        .collect::<Vec<_>>()
        .soft_encode(&mut store);
    unsafe { *(*encoded.as_ptr()).as_ptr() = co3::primitives::CBool::TRUE };
    drop(unsafe { encoded.into_box() }.unwrap());
    store.sync().unwrap();
    assert_eq!(values, [true, false]);
}

#[test]
fn stable_vec_decode_checks_validity_before_reusing_allocation() {
    use co3::{boxed::CWideBox, primitives::CBool};

    let source = CWideBox::from_box(Box::<[CBool]>::from([CBool(1), CBool(0)]));
    let original = source.as_ptr().cast::<bool>();
    let decoded = unsafe { Vec::<bool>::soft_decode(source, &mut ()) }.unwrap();
    assert_eq!(decoded.as_ptr(), original);
    assert_eq!(decoded, [true, false]);

    let invalid = CWideBox::from_box(Box::<[CBool]>::from([CBool(1), CBool(2)]));
    assert!(unsafe { Vec::<bool>::soft_decode(invalid, &mut ()) }.is_none());
    assert!(unsafe { Vec::<bool>::soft_decode(CWideBox::null(), &mut ()) }.is_none());

    let source = CWideBox::from_box(Box::<[CBool]>::from([CBool(1)]));
    let original = source.as_ptr().cast::<bool>();
    let decoded = unsafe { Vec::<bool>::soft_decode_unchecked(source, &mut ()) };
    assert_eq!(decoded.as_ptr(), original);
    assert_eq!(decoded, [true]);
}
