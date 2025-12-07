use co3::ExternC;

type WrapperInner = u32;

#[derive(ExternC)]
#[mineral(
    NICHE_VALUE = 0,
    unsafe(is_valid = |target|
        target != 0
    )
)]
#[repr(transparent)]
pub struct Wrapper(WrapperInner);

/// Take exclusive reference to a structure that is not-robust structure, for which it cannot
/// be guaranteed that the caller of the function will not set it to a trap representation.
#[co3::carbonate]
pub fn take_non_robust_ref_mut(_ffi_struct: &mut Wrapper) {}

fn main() {}
