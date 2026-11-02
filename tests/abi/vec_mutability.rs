use core::cell::Cell;

use co3::stored::Store;

#[test]
fn stable_interior_vec_direct_encoding_transfers_allocation() {
    let values = Box::<[Cell<bool>]>::from([Cell::new(true), Cell::new(false)]).into_vec();
    let original = values.as_ptr();
    let mut encode_store = ();
    let encoded = unsafe { co3::soft_encode_unchecked(values, &mut encode_store) };
    assert_eq!(encoded.as_ptr().cast::<Cell<bool>>(), original.cast_mut());
    let mut store = ();
    let decoded = unsafe { co3::soft_decode::<Vec<Cell<bool>>>(encoded, &mut store) }.unwrap();
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
    let encoded = co3::soft_encode(vec![Cell::new((3_u8,)), Cell::new((7,))], &mut encode_store);
    let mut decode_store = Default::default();
    let decoded =
        unsafe { co3::soft_decode::<Vec<Cell<(u8,)>>>(encoded, &mut decode_store) }.unwrap();
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
    let encoded =
        unsafe { co3::soft_encode_unchecked(vec![Cell::new((9_u8,))], &mut encode_store) };
    let mut decode_store = Default::default();
    let decoded =
        unsafe { co3::soft_decode_unchecked::<Vec<Cell<(u8,)>>>(encoded, &mut decode_store) };
    assert_eq!(decoded[0].get(), (9,));
    decode_store.sync().unwrap();
    encode_store.sync().unwrap();
}

#[test]
fn robust_vec_checked_encoding_preserves_values_and_capacity() {
    let mut values = Vec::<u32>::with_capacity(12);
    values.extend([3, 7]);
    let capacity = values.capacity();
    let mut encode_store = Default::default();
    let encoded = co3::soft_encode(values, &mut encode_store);
    assert_eq!(encoded.capacity(), capacity);
    let decoded = unsafe { encoded.into_vec() }.unwrap();
    assert_eq!(decoded.capacity(), capacity);
    assert_eq!(decoded, [3, 7]);
    encode_store.sync().unwrap();

    let values = Box::<[Cell<u8>]>::from([Cell::new(3), Cell::new(7)]).into_vec();
    let mut encode_store = Default::default();
    let encoded = co3::soft_encode(values, &mut encode_store);
    let mut store = ();
    let decoded = unsafe { co3::soft_decode::<Vec<Cell<u8>>>(encoded, &mut store) }.unwrap();
    assert_eq!(decoded.iter().map(Cell::get).collect::<Vec<_>>(), [3, 7]);
    store.sync().unwrap();
    encode_store.sync().unwrap();
}

#[test]
fn nonrobust_vec_keeps_element_writeback_stores() {
    let mut values = [false, false];
    let mut store = Default::default();
    let encoded = co3::soft_encode(values.iter_mut().collect::<Vec<_>>(), &mut store);
    unsafe { *(*encoded.as_ptr()).as_ptr() = co3::primitives::CBool::TRUE };
    drop(unsafe { encoded.into_vec() }.unwrap());
    store.sync().unwrap();
    assert_eq!(values, [true, false]);
}

#[test]
fn stable_vec_decode_checks_validity_before_reusing_allocation() {
    use co3::{primitives::CBool, vec::CVec};

    let source = CVec::<bool>::from_vec(vec![CBool(1), CBool(0)]);
    let original = source.as_ptr().cast::<bool>();
    let mut store = ();
    let decoded = unsafe { co3::soft_decode::<Vec<bool>>(source, &mut store) }.unwrap();
    assert_eq!(decoded.as_ptr(), original);
    assert_eq!(decoded, [true, false]);

    let invalid = CVec::<bool>::from_vec(vec![CBool(1), CBool(2)]);
    assert!(unsafe { co3::soft_decode::<Vec::<bool>>(invalid, &mut Default::default()) }.is_none());
    assert!(
        unsafe { co3::soft_decode::<Vec::<bool>>(CVec::null(), &mut Default::default()) }.is_none()
    );

    let source = CVec::<bool>::from_vec(vec![CBool(1)]);
    let original = source.as_ptr().cast::<bool>();
    let decoded =
        unsafe { co3::soft_decode_unchecked::<Vec<bool>>(source, &mut Default::default()) };
    assert_eq!(decoded.as_ptr(), original);
    assert_eq!(decoded, [true]);
}
