use core::cell::UnsafeCell;

use co3::{borrow::Borrowed, primitives::CBool, reference::CSoftRefMut, stored::Store};

#[test]
fn mutable_reference_decode_validates() {
    let mut carrier = CBool(1);
    let mut store = ();
    let decoded = unsafe {
        co3::soft_decode::<Borrowed<&mut bool>>(
            CSoftRefMut::from_mut(&mut carrier).into(),
            &mut store,
        )
    }
    .unwrap();
    assert!(*Borrowed::into_inner(decoded));
    assert_eq!(store.sync(), Some(()));
    let mut invalid = CBool(2);
    let mut store = ();
    assert!(
        unsafe {
            co3::soft_decode::<Borrowed<&mut bool>>(
                CSoftRefMut::from_mut(&mut invalid).into(),
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
            CSoftRefMut::from_mut(&mut value).into(),
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
            CSoftRefMut::from_raw(core::ptr::from_ref(&value).cast_mut()).into(),
            &mut store,
        )
    }
    .unwrap();
    assert!(core::ptr::eq(
        core::ptr::from_ref(Borrowed::into_inner(decoded)),
        &value
    ));
}
