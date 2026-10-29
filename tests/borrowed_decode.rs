use co3::{
    borrow::Borrowed,
    primitives::CBool,
    reference::CRefMut,
    stored::{DecodeMove, Store},
};
use core::cell::UnsafeCell;

#[test]
fn mutable_reference_decode_validates() {
    let mut carrier = CBool(1);
    let mut store = Default::default();
    let decoded = unsafe {
        <Borrowed<&mut bool>>::soft_decode(
            Borrowed::new(CRefMut::from_mut(&mut carrier)),
            &mut store,
        )
    }
    .unwrap();
    assert!(*Borrowed::into_inner(decoded));
    assert_eq!(store.sync(), Some(()));
    let mut invalid = CBool(2);
    let mut store = Default::default();
    assert!(
        unsafe {
            <Borrowed<&mut bool>>::soft_decode(
                Borrowed::new(CRefMut::from_mut(&mut invalid)),
                &mut store,
            )
        }
        .is_none()
    );
}

#[test]
fn mutable_reference_unchecked_decode_preserves_pointer() {
    let mut value = 42u32;
    let pointer = core::ptr::from_mut(&mut value);
    let mut store = Default::default();
    let decoded = unsafe {
        <Borrowed<&mut u32>>::soft_decode_unchecked(
            Borrowed::new(CRefMut::from_mut(&mut value)),
            &mut store,
        )
    };
    assert_eq!(core::ptr::from_mut(Borrowed::into_inner(decoded)), pointer);
}

#[test]
fn shared_interior_reference_decodes_directly() {
    let value = UnsafeCell::new(7u8);
    let mut store = Default::default();
    let decoded = unsafe {
        <Borrowed<&UnsafeCell<u8>>>::soft_decode(
            Borrowed::new(CRefMut::from_raw(core::ptr::from_ref(&value).cast_mut())),
            &mut store,
        )
    }
    .unwrap();
    assert!(core::ptr::eq(
        core::ptr::from_ref(Borrowed::into_inner(decoded)),
        &value
    ));
}
