use core::mem::MaybeUninit;

use co3::{
    borrow::{borrow_cast, borrow_cast_mut},
    boxed::CBox,
    reference::{CRef, CRefMut},
};

#[test]
fn initialized_carrier_preserves_its_pointer_in_both_views() {
    let owner = CBox::from_box(Box::new(42_u32));
    let pointer = owner.as_ptr();

    let shared: MaybeUninit<CRef<u32>> = borrow_cast(MaybeUninit::new(owner));
    let exclusive: MaybeUninit<CRefMut<u32, true>> = borrow_cast_mut(MaybeUninit::new(owner));

    // Both views came from initialized storage. Only the original carrier owns the allocation.
    assert_eq!(
        unsafe { shared.assume_init() }.as_ptr(),
        pointer.cast_const()
    );
    assert_eq!(unsafe { exclusive.assume_init() }.as_ptr(), pointer);
    assert_eq!(*unsafe { owner.into_box() }.unwrap(), 42);
}

#[test]
fn uninitialized_carrier_can_be_cast_without_initializing_it() {
    let _: MaybeUninit<CRef<u32>> = borrow_cast(MaybeUninit::<CBox<u32>>::uninit());
    let _: MaybeUninit<CRefMut<u32, true>> = borrow_cast_mut(MaybeUninit::<CBox<u32>>::uninit());
}
