use co3::ExternC;

type WrapperInner = u32;

#[derive(ExternC)]
#[mineral(
    NICHE_VALUE = 0,
    unsafe(is_valid = |target: &Self::Target|
        *target != 0
    )
)]
#[repr(transparent)]
pub struct Wrapper(WrapperInner);

#[co3::carbonate]
pub fn return_non_robust_ref_mut<'a>() -> &'a mut Wrapper {
    unimplemented!()
}

#[co3::carbonate]
pub fn take_non_robust_ref_mut(_ffi_struct: &mut Wrapper) {}

fn main() {}
