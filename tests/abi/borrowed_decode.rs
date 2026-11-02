use co3::{borrow::Borrowed, primitives::Bool, reference::CRefMut, stored::Store};
use core::cell::UnsafeCell;

#[test]
fn mutable_reference_decode_validates() {
    let mut carrier = Bool(1);
    let mut store = ();
    let decoded = unsafe {
        co3::soft_decode::<Borrowed<&mut bool>>(CRefMut::from_mut(&mut carrier).into(), &mut store)
    }
    .unwrap();
    assert!(*Borrowed::into_inner(decoded));
    assert_eq!(store.sync(), Some(()));
    let mut invalid = Bool(2);
    let mut store = ();
    assert!(
        unsafe {
            co3::soft_decode::<Borrowed<&mut bool>>(
                CRefMut::from_mut(&mut invalid).into(),
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
    let mut store = ();
    let decoded = unsafe {
        co3::soft_decode_unchecked::<Borrowed<&mut u32>>(
            CRefMut::from_mut(&mut value).into(),
            &mut store,
        )
    };
    assert_eq!(core::ptr::from_mut(Borrowed::into_inner(decoded)), pointer);
}

#[test]
fn shared_interior_reference_decodes_directly() {
    let value = UnsafeCell::new(7u8);
    let mut store = ();
    let decoded = unsafe {
        co3::soft_decode::<Borrowed<&UnsafeCell<u8>>>(
            CRefMut::from_raw(core::ptr::from_ref(&value).cast_mut()).into(),
            &mut store,
        )
    }
    .unwrap();
    assert!(core::ptr::eq(
        core::ptr::from_ref(Borrowed::into_inner(decoded)),
        &value
    ));
}
