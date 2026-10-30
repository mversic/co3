use co3::borrow::{Borrow, Borrowed};
use co3::stored::Store;

#[test]
fn owned_containers_preserve_their_type_and_allocation_when_borrowed() {
    fn restore<T: Borrow<Borrowed = Borrowed<T>>>(value: T) -> T {
        T::from_borrow(value.borrow())
    }

    let boxed = Box::new(7_u8);
    let original = core::ptr::from_ref(&*boxed);
    assert_eq!(core::ptr::from_ref(&*restore(boxed)), original);

    let values = vec![1_u8, 2];
    let original = values.as_ptr();
    let values = restore(values);
    assert_eq!(values.as_ptr(), original);
    let values = restore(co3::vec::Vec::from(values));
    assert_eq!(values.as_ptr(), original);

    let text = String::from("owned");
    let original = text.as_ptr();
    let restored = restore(text);
    assert_eq!(restored.as_ptr(), original);

    let text = std::ffi::CString::new("owned").unwrap();
    let original = text.as_ptr();
    let restored = restore(text);
    assert_eq!(restored.as_ptr(), original);
}

#[test]
fn borrowed_mut_reference_keeps_its_marker() {
    let mut value = 7u8;
    let borrowed: Borrowed<&mut u8> = (&mut value).borrow();
    let mut store = ();
    let encoded = co3::soft_encode(borrowed, &mut store);
    assert_eq!(unsafe { *Borrowed::into_inner(encoded).as_ptr() }, 7);
    assert_eq!(store.sync(), Some(()));
    assert_eq!(value, 7);
}

#[test]
fn borrowed_box_keeps_owner_until_sync() {
    let value = Box::new(9u8);
    let borrowed: Borrowed<Box<u8>> = value.borrow();
    let mut store = Default::default();
    let encoded = co3::soft_encode(borrowed, &mut store);
    assert_eq!(unsafe { *Borrowed::into_inner(encoded).as_ptr() }, 9);
    assert_eq!(store.sync(), Some(()));
}

#[test]
fn shared_interior_reference_encodes_directly() {
    let value = core::cell::UnsafeCell::new(7u8);
    let borrowed = (&value).borrow();
    let encoded = co3::soft_encode(borrowed, &mut Default::default());
    assert_eq!(Borrowed::into_inner(encoded).as_ptr(), value.get().cast());
}

#[test]
fn owned_decode_clones_without_taking_ownership() {
    let value = Box::new(11u8);
    let original = core::ptr::from_ref(&*value);
    let mut encode_store = Default::default();
    let carrier = co3::soft_encode(value.borrow(), &mut encode_store);
    let mut decode_store = Default::default();
    let decoded =
        unsafe { co3::soft_decode::<Borrowed<Box<u8>>>(carrier, &mut decode_store) }.unwrap();
    let cloned = Borrowed::into_inner(decoded);
    assert_eq!(*cloned, 11);
    assert_ne!(core::ptr::from_ref(&*cloned), original);
    assert_eq!(decode_store.sync(), Some(()));
    assert_eq!(encode_store.sync(), Some(()));
    assert_eq!(*cloned, 11);
}

#[test]
fn shared_references_with_reachable_interior_are_wrapped() {
    use core::cell::UnsafeCell;
    let plain = 5u8;
    let identity: &u8 = (&plain).borrow();
    assert!(core::ptr::eq(identity, &plain));

    let cell = UnsafeCell::new(7u8);
    let inner = &cell;
    let wrapped: Borrowed<&&UnsafeCell<u8>> = (&inner).borrow();
    let encoded = co3::soft_encode(wrapped, &mut Default::default());
    let mut store = ();
    let decoded =
        unsafe { co3::soft_decode::<Borrowed<&&UnsafeCell<u8>>>(encoded, &mut store) }.unwrap();
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
