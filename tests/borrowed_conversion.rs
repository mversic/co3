use co3::borrow::{Borrow, Borrowed};
use co3::stored::{EncodeMove, Store};

#[test]
fn borrowed_mut_reference_keeps_its_marker() {
    let mut value = 7u8;
    let borrowed: Borrowed<&mut u8> = (&mut value).borrow();
    let mut store = Default::default();
    let encoded = borrowed.soft_encode(&mut store);
    assert_eq!(unsafe { *Borrowed::into_inner(encoded).as_ptr() }, 7);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, 7);
}

#[test]
fn borrowed_box_keeps_owner_until_sync() {
    let value = Box::new(9u8);
    let borrowed: Borrowed<Box<u8>> = value.borrow();
    let mut store = Default::default();
    let encoded = borrowed.soft_encode(&mut store);
    assert_eq!(unsafe { *Borrowed::into_inner(encoded).as_ptr() }, 9);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn shared_interior_reference_encodes_directly() {
    let value = core::cell::UnsafeCell::new(7u8);
    let borrowed = (&value).borrow();
    let encoded = borrowed.soft_encode(&mut ());
    assert_eq!(Borrowed::into_inner(encoded).as_ptr(), value.get().cast());
}

#[test]
fn owned_decode_clones_without_taking_ownership() {
    use co3::stored::DecodeOwned;
    let value = Box::new(11u8);
    let original = core::ptr::from_ref(&*value);
    let mut encode_store = Default::default();
    let carrier = value.borrow().soft_encode(&mut encode_store);
    let mut decode_store = Default::default();
    let decoded = unsafe { <Borrowed<Box<u8>>>::soft_decode(carrier, &mut decode_store) }.unwrap();
    let cloned = Borrowed::into_inner(decoded);
    assert_eq!(*cloned, 11);
    assert_ne!(core::ptr::from_ref(&*cloned), original);
    assert_eq!(decode_store.sync(), Some(()));
    assert_eq!(encode_store.sync(), Some(()));
    assert_eq!(*cloned, 11);
}

#[test]
fn shared_references_with_reachable_interior_are_wrapped() {
    use co3::stored::DecodeOwned;
    use core::cell::UnsafeCell;
    let plain = 5u8;
    let identity: &u8 = (&plain).borrow();
    assert!(core::ptr::eq(identity, &plain));

    let cell = UnsafeCell::new(7u8);
    let inner = &cell;
    let wrapped: Borrowed<&&UnsafeCell<u8>> = (&inner).borrow();
    let encoded = wrapped.soft_encode(&mut ());
    let mut store = ();
    let decoded =
        unsafe { <Borrowed<&&UnsafeCell<u8>>>::soft_decode(encoded, &mut store) }.unwrap();
    assert!(core::ptr::eq(
        core::ptr::from_ref(Borrowed::into_inner(decoded)),
        &inner
    ));

    let cell = UnsafeCell::new(false);
    let inner = &cell;
    let wrapped: Borrowed<&&UnsafeCell<bool>> = (&inner).borrow();
    assert!(core::ptr::eq(
        core::ptr::from_ref(Borrowed::into_inner(wrapped)),
        &inner
    ));
}
