use co3::ExternC;

type WrapperInner = u32;

#[derive(ExternC)]
#[repr(transparent)]
pub struct Wrapper(WrapperInner);

co3::mineral! {
    unsafe impl Transparent for Wrapper {
        type Target = WrapperInner;

        const NICHE_VALUE: Self::CType = 0;
        fn is_valid(target: &Self::Target) -> bool {
            *target != 0
        }
    }
}

/// Take exclusive reference to a structure that is not-robust structure, for which it cannot
/// be guaranteed that the caller of the function will not set it to a trap representation.
#[co3::carbonate]
pub fn take_non_robust_ref_mut(_ffi_struct: &mut Wrapper) {}

fn main() {}
