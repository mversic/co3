#![cfg(all(feature = "derive", feature = "alloc"))]

use co3::{
    ReprC,
    borrow::{Borrow, Borrowed},
    rust_spec::RustSpec,
};
use static_assertions::assert_type_eq_all;

// A downstream field can support representation without structural borrowing.
#[derive(RustSpec)]
struct Field(u8);
impl ReprC for Field {
    type CType = u8;
}

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_drop)]
struct Owner<T>(T);
impl<T> Drop for Owner<T> {
    fn drop(&mut self) {}
}

#[derive(RustSpec, ReprC)]
#[rust_spec(custom_drop)]
enum EnumOwner<T> {
    Value(T),
    Empty,
}
impl<T> Drop for EnumOwner<T> {
    fn drop(&mut self) {}
}

#[derive(RustSpec, ReprC)]
struct Automatic(Box<u8>);

#[test]
fn custom_drop_owners_keep_their_borrowing_contract() {
    assert_type_eq_all!(<Owner<Field> as Borrow>::Borrowed, Borrowed<Owner<Field>>);
    assert_type_eq_all!(
        <EnumOwner<Field> as Borrow>::Borrowed,
        Borrowed<EnumOwner<Field>>
    );
    let restored = Owner::from_borrow(Owner(Field(7)).borrow());
    assert_eq!(restored.0.0, 7);
}

#[test]
fn automatic_drop_still_borrows_structurally() {
    let view: AutomaticView = Automatic(Box::new(9)).borrow();
    let restored = Automatic::from_borrow(view);
    assert_eq!(*restored.0, 9);
}

#[derive(RustSpec, ReprC)]
struct GenericTail<T: ?Sized>(u8, T);

#[test]
fn generic_tail_borrowing_requires_a_sized_tail() {
    assert_type_eq_all!(<GenericTail<u8> as Borrow>::Borrowed, GenericTail<u8>);
    static_assertions::assert_not_impl_any!(GenericTail<[u8]>: Borrow);
    let view = GenericTail(3, Box::new(7)).borrow();
    let restored: GenericTail<Box<u8>> = GenericTail::from_borrow(view);
    assert_eq!((restored.0, *restored.1), (3, 7));
}
