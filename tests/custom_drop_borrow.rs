use co3::{
    ReprC,
    borrow::{Borrow, Borrowed},
    boxed::CBox,
    rust_spec::RustSpec,
};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[rust_spec(custom_drop)]
struct Custom(CBox<u8>);

impl Drop for Custom {
    fn drop(&mut self) {}
}

#[test]
fn custom_drop_borrows_whole_while_carriers_borrow_as_self() {
    static_assertions::assert_type_eq_all!(<CBox<u8> as Borrow>::Borrowed, CBox<u8>);
    static_assertions::assert_type_eq_all!(<CCustom as Borrow>::Borrowed, CCustom);
    static_assertions::assert_type_eq_all!(<Custom as Borrow>::Borrowed, Borrowed<Custom>);
}

#[test]
fn pointer_carriers_borrow_without_borrowing_the_pointee() {
    use co3::boxed::{CBox, CWideBox};
    use co3::vec::CVec;
    use core::cell::{Cell, UnsafeCell};

    fn identity<C: Borrow<Borrowed = C>>(carrier: C) -> C {
        C::from_borrow(carrier.borrow())
    }

    assert!(identity(CBox::<UnsafeCell<u8>>::null()).as_ptr().is_null());
    assert!(identity(CBox::<UnsafeCell<u8>>::null()).as_ptr().is_null());
    assert!(identity(CWideBox::<[UnsafeCell<u8>]>::null()).as_ptr().is_null());
    assert!(identity(CWideBox::<[UnsafeCell<u8>]>::null()).as_ptr().is_null());
    assert!(identity(CVec::<UnsafeCell<u8>>::null()).as_ptr().is_null());
    assert!(identity(CBox::<Borrowed<u8>>::null()).as_ptr().is_null());
    assert!(identity(CVec::<Borrowed<u8>>::null()).as_ptr().is_null());
    assert_eq!(identity(Cell::new(7_u8)).into_inner(), 7);
}
