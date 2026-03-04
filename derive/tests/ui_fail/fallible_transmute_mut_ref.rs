use co3::{ReprC, export};

type WrapperInner = u32;

#[derive(ReprC)]
#[reprC(
    NICHE_VALUE = 0,
    unsafe(is_valid = |target: &Self::Target|
        *target != 0
    )
)]
#[repr(transparent)]
pub struct Wrapper(WrapperInner);

#[export("C")]
pub extern "C" fn return_non_robust_ref_mut<'a>() -> &'a mut Wrapper {
    unimplemented!()
}

#[export("C")]
pub extern "C" fn take_non_robust_ref_mut(_ffi_struct: &mut Wrapper) {}

fn main() {}
