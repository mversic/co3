use co3::{ReprC, borrow::{Borrow, Borrowed}, boxed::CBox, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
#[rust_spec(custom_drop)]
struct Custom(CBox<u8>);

impl Drop for Custom {
    fn drop(&mut self) {}
}

#[test]
fn custom_drop_borrows_whole_without_field_borrow() {
    static_assertions::assert_not_impl_any!(CBox<u8>: Borrow);
    static_assertions::assert_type_eq_all!(<Custom as Borrow>::Borrowed, Borrowed<Custom>);
}

#[test]
fn custom_drop_carrier_cast_preserves_the_wrapper() {
    use co3::borrow::{BorrowCast, BorrowCastMut};
    static_assertions::assert_type_eq_all!(<CCustom as BorrowCast>::AsConst, Borrowed<CCustom>);
    static_assertions::assert_not_impl_any!(CCustom: BorrowCastMut);
}
