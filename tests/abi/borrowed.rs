use std::sync::atomic::{AtomicUsize, Ordering};

use co3::{ReprC, borrow::Borrow, rust_spec::RustSpec};
use static_assertions::assert_type_eq_all;

#[test]
fn retained_owners_expose_borrowed_views_without_changing_the_carrier() {
    use co3::{borrow::Borrowed, stored::Store};
    assert_type_eq_all!(<Box<u32> as Borrow>::Borrowed, Borrowed<Box<u32>>);
    assert_type_eq_all!(<Vec<u32> as Borrow>::Borrowed, Borrowed<Vec<u32>>);
    assert_type_eq_all!(
        <co3::vec::Vec<u32> as Borrow>::Borrowed,
        Borrowed<co3::vec::Vec<u32>>
    );
    assert_type_eq_all!(
        <std::ffi::CString as Borrow>::Borrowed,
        Borrowed<std::ffi::CString>
    );
    assert_type_eq_all!(<String as Borrow>::Borrowed, Borrowed<String>);
    let value = Box::new(42_u32);
    let original = core::ptr::from_ref(&*value);
    let mut store = Default::default();
    let encoded = co3::soft_encode(value.borrow(), &mut store);
    assert_eq!(
        Borrowed::into_inner(encoded).as_ptr().cast_const(),
        original
    );
    let mut decode_store = Default::default();
    let decoded =
        unsafe { co3::soft_decode::<Borrowed<Box<u32>>>(encoded, &mut decode_store) }.unwrap();
    let cloned = Borrowed::into_inner(decoded);
    assert_eq!(*cloned, 42);
    assert_ne!(core::ptr::from_ref(&*cloned), original);
    assert_eq!(decode_store.sync(), Some(()));
    assert_eq!(store.sync(), Some(()));
    assert_eq!(
        String::from_borrow(String::from("borrowed").borrow()),
        "borrowed"
    );
    assert_eq!(Vec::<u32>::from_borrow(vec![1_u32, 2].borrow()), [1, 2]);
}

static DROPS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, RustSpec, ReprC)]
#[repr(transparent)]
#[rust_spec(custom_drop)]
struct RetainedDrop(u32);

impl Drop for RetainedDrop {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn custom_drop_keeps_the_original_alive_and_clones_on_restore() {
    use co3::{borrow::Borrowed, stored::Store};
    assert_type_eq_all!(<RetainedDrop as Borrow>::Borrowed, Borrowed<RetainedDrop>);
    let mut store = Default::default();
    let encoded = co3::soft_encode(RetainedDrop(7).borrow(), &mut store);
    let mut decode_store = Default::default();
    let cloned = Borrowed::into_inner(
        unsafe { co3::soft_decode::<Borrowed<RetainedDrop>>(encoded, &mut decode_store) }.unwrap(),
    );
    assert_eq!(cloned.0, 7);
    assert_eq!(DROPS.load(Ordering::Relaxed), 0);
    drop(cloned);
    assert_eq!(DROPS.load(Ordering::Relaxed), 1);
    assert_eq!(decode_store.sync(), Some(()));
    assert_eq!(store.sync(), Some(()));
    assert_eq!(DROPS.load(Ordering::Relaxed), 2);
}
