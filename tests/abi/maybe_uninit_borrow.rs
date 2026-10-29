use core::mem::MaybeUninit;

use co3::{borrow::Borrow, boxed::CBox};

#[test]
fn initialized_carrier_borrow_preserves_its_pointer() {
    let owner = CBox::from_box(Box::new(42_u32));
    let pointer = owner.as_ptr();
    let borrowed: MaybeUninit<CBox<u32>> = MaybeUninit::new(owner).borrow();

    // The fixture initialized the value; borrowing must not consume its allocation.
    assert_eq!(unsafe { borrowed.assume_init() }.as_ptr(), pointer);
    assert_eq!(*unsafe { owner.into_box() }.unwrap(), 42);
}

#[test]
fn uninitialized_carrier_can_be_borrowed_without_initializing_it() {
    let borrowed: MaybeUninit<CBox<u32>> = MaybeUninit::uninit().borrow();
    let _: MaybeUninit<CBox<u32>> = Borrow::from_borrow(borrowed);
}
