use core::cell::Cell;

#[test]
fn stable_interior_box_direct_round_trip_preserves_allocation() {
    let value = Box::new(Cell::new(true));
    let original = core::ptr::from_ref(&*value);
    let encoded = unsafe { co3::soft_encode_unchecked(value, &mut Default::default()) };
    assert_eq!(encoded.as_ptr().cast::<Cell<bool>>(), original.cast_mut());
    let decoded =
        unsafe { co3::soft_decode::<Box<Cell<bool>>>(encoded, &mut Default::default()) }.unwrap();
    assert_eq!(core::ptr::from_ref(&*decoded), original);
    assert!(decoded.get());
}

#[test]
fn stable_interior_slice_box_direct_round_trip_preserves_allocation() {
    let value = Box::new([Cell::new(true), Cell::new(false)]) as Box<[Cell<bool>]>;
    let original = value.as_ptr();
    let encoded = unsafe { co3::soft_encode_unchecked(value, &mut Default::default()) };
    assert_eq!(encoded.as_ptr().cast::<Cell<bool>>(), original.cast_mut());
    let decoded =
        unsafe { co3::soft_decode::<Box<[Cell<bool>]>>(encoded, &mut Default::default()) }.unwrap();
    assert_eq!(decoded.as_ptr(), original);
    assert_eq!(
        decoded.iter().map(Cell::get).collect::<Vec<_>>(),
        [true, false]
    );
}

#[test]
fn unstable_interior_box_has_no_public_conversion() {
    static_assertions::assert_not_impl_any!(Box<Cell<(u8,)>>: co3::Encode, co3::Decode<'static>);
}

#[test]
fn interior_box_validation_checks_null_and_pointee() {
    use co3::{boxed::CBox, transmute::AbiCompatibleValidate};

    assert!(!unsafe { Box::<Cell<bool>>::is_valid(&CBox::null()) });
    let carrier = unsafe { co3::encode_unchecked(Box::new(Cell::new(true))) };
    assert!(unsafe { Box::<Cell<bool>>::is_valid(&carrier) });
    drop(unsafe { co3::soft_decode::<Box<Cell<bool>>>(carrier, &mut Default::default()) }.unwrap());
}
