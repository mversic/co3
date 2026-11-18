use core::cell::Cell;

use co3::boxed::RustBox;

#[test]
fn stable_interior_box_direct_round_trip_preserves_allocation() {
    let value = RustBox::new(Cell::new(true));
    let original = core::ptr::from_ref(&*value);
    let encoded = unsafe { co3::soft_encode_unchecked(value, &mut Default::default()) };
    assert_eq!(encoded.as_ptr().cast::<Cell<bool>>(), original.cast_mut());
    let decoded =
        unsafe { co3::soft_decode::<RustBox<Cell<bool>>>(encoded, &mut Default::default()) }
            .unwrap();
    assert_eq!(core::ptr::from_ref(&*decoded), original);
    assert!(decoded.get());
}

#[test]
fn stable_interior_slice_box_direct_round_trip_preserves_allocation() {
    let value = RustBox::new([Cell::new(true), Cell::new(false)]) as RustBox<[Cell<bool>]>;
    let original = value.as_ptr();
    let encoded = unsafe { co3::soft_encode_unchecked(value, &mut Default::default()) };
    assert_eq!(encoded.as_ptr().cast::<Cell<bool>>(), original.cast_mut());
    let decoded =
        unsafe { co3::soft_decode::<RustBox<[Cell<bool>]>>(encoded, &mut Default::default()) }
            .unwrap();
    assert_eq!(decoded.as_ptr(), original);
    assert_eq!(
        decoded.iter().map(Cell::get).collect::<Vec<_>>(),
        [true, false]
    );
}

#[test]
fn unstable_interior_box_has_no_public_conversion() {
    static_assertions::assert_not_impl_any!(RustBox<Cell<(u8,)>>: co3::convert::Encode, co3::convert::Decode<'static>);
}

#[test]
fn interior_box_validation_checks_null_and_pointee() {
    use co3::{boxed::Box, transmute::AbiCompatibleValidate};

    assert!(!unsafe { RustBox::<Cell<bool>>::is_valid(&Box::null()) });
    let carrier = unsafe { co3::encode_unchecked(RustBox::new(Cell::new(true))) };
    assert!(unsafe { RustBox::<Cell<bool>>::is_valid(&carrier) });
    drop(
        unsafe { co3::soft_decode::<RustBox<Cell<bool>>>(carrier, &mut Default::default()) }
            .unwrap(),
    );
}
