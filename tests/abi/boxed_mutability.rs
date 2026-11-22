use core::cell::Cell;

use co3::boxed::Box;

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
    static_assertions::assert_not_impl_any!(Box<Cell<(u8,)>>: co3::convert::Encode, co3::convert::Decode<'static>);
}

#[test]
fn recursive_box_keeps_child_writeback_until_sync() {
    #[derive(co3::rust_spec::RustSpec, co3::ReprC)]
    #[repr(C)]
    struct Node {
        flag: &'static mut bool,
        next: Option<Box<Self>>,
    }

    let flags = Box::into_raw(Box::new([false, false]));
    // Both flags remain exclusively borrowed by the conversion until sync.
    let [root, child] = unsafe { &mut *flags };
    let node = Node {
        flag: root,
        next: Some(Box::new(Node {
            flag: child,
            next: None,
        })),
    };
    let mut store = Default::default();
    let carrier = co3::soft_encode(node, &mut store);
    // Encoding transferred ownership of the child allocation.
    let child = unsafe { carrier.next.0.0.into_box() }.unwrap();
    unsafe { *child.flag.as_ptr() = co3::encode(true) };
    drop(child);
    co3::convert::Store::sync(store).unwrap();
    assert_eq!(*unsafe { Box::from_raw(flags) }, [false, true]);
}
