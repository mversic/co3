use co3::{
    ReprC,
    borrow::{Borrow, FromBorrow},
    rust_spec::RustSpec,
};
use static_assertions::assert_type_eq_all;
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn retained_owners_expose_borrowed_views_without_changing_the_carrier() {
    assert_type_eq_all!(<Box<u32> as Borrow>::Borrowed<'static>, &'static u32);
    assert_type_eq_all!(<Vec<u32> as Borrow>::Borrowed<'static>, &'static [u32]);
    assert_type_eq_all!(<String as Borrow>::Borrowed<'static>, &'static str);
    assert_type_eq_all!(
        <Option<Box<u32>> as Borrow>::Borrowed<'static>,
        Option<&'static u32>
    );
    assert_type_eq_all!(
        <<Box<u32> as Borrow>::Borrowed<'static> as ReprC>::CType,
        co3::reference::CRef<u32>
    );

    let value = Box::new(42_u32);
    let original = core::ptr::from_ref(&*value);
    let mut owner = Default::default();
    let mut store = Default::default();
    let encoded = unsafe { co3::soft_encode_unchecked(value.borrow(&mut owner), &mut store) };
    assert_eq!(encoded.as_ptr(), original);
    let decoded = unsafe { co3::decode::<&u32>(encoded) }.unwrap();
    let cloned = Box::<u32>::from_borrow(decoded);
    assert_eq!(*cloned, 42);
    assert_ne!(core::ptr::from_ref(&*cloned), original);
    co3::stored::Store::sync(store).unwrap();

    let mut string_owner = Default::default();
    let borrowed = String::from("borrowed").borrow(&mut string_owner);
    assert_eq!(String::from_borrow(borrowed), "borrowed");
    let mut vec_owner = Default::default();
    let borrowed = vec![1_u32, 2].borrow(&mut vec_owner);
    assert_eq!(Vec::<u32>::from_borrow(borrowed), [1, 2]);

    let encoded = co3::encode(None::<&u32>);
    assert!(
        unsafe { co3::decode::<Option<&u32>>(encoded) }
            .unwrap()
            .is_none()
    );
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
    assert_type_eq_all!(
        <RetainedDrop as Borrow>::Borrowed<'static>,
        &'static RetainedDrop
    );
    let mut owner = Default::default();
    let mut store = Default::default();
    let encoded =
        unsafe { co3::soft_encode_unchecked(RetainedDrop(7).borrow(&mut owner), &mut store) };
    let decoded = unsafe { co3::decode::<&RetainedDrop>(encoded) }.unwrap();
    let cloned = RetainedDrop::from_borrow(decoded);
    assert_eq!(cloned.0, 7);
    assert_eq!(DROPS.load(Ordering::Relaxed), 0);
    drop(cloned);
    assert_eq!(DROPS.load(Ordering::Relaxed), 1);
    co3::stored::Store::sync(store).unwrap();
    assert_eq!(DROPS.load(Ordering::Relaxed), 1);
    drop(owner);
    assert_eq!(DROPS.load(Ordering::Relaxed), 2);
}
