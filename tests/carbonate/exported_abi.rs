use std::mem::MaybeUninit;

use co3::{ExternC, FfiReturn, out_ptr::OutPtrRead as _};
use webassembly_test::webassembly_test;

trait AmbiguousX<T> {
    fn ambiguous() -> Ambiguous;
}

trait AmbiguousY {
    fn ambiguous() -> Ambiguous;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(u8)]
pub enum Ambiguous {
    AmbiguousX,
    AmbiguousY,
    Inherent,
    None,
}

#[derive(Clone, Copy, ExternC)]
#[mineral(opaque)]
pub(crate) struct OpaqueStruct<T>(T);

#[co3::carbonate]
impl AmbiguousX<u64> for OpaqueStruct<u64> {
    fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

#[co3::carbonate]
impl AmbiguousX<u32> for OpaqueStruct<u32> {
    fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

#[co3::carbonate]
impl AmbiguousY for OpaqueStruct<u64> {
    fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousY
    }
}

#[co3::carbonate]
impl AmbiguousY for OpaqueStruct<u32> {
    fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousY
    }
}

#[co3::carbonate]
impl OpaqueStruct<u64> {
    pub fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

#[co3::carbonate]
impl OpaqueStruct<u32> {
    pub fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

#[test]
#[webassembly_test]
fn exported_abi() {
    let mut output = MaybeUninit::new(Ambiguous::None as _);

    unsafe extern "C" {
        fn carbonate_OpaqueStruct_u32_ambiguous(output: *mut u8) -> FfiReturn;
        fn carbonate_AmbiguousX_u32_OpaqueStruct_u32_ambiguous(output: *mut u8) -> FfiReturn;
        fn carbonate_AmbiguousX_u64_OpaqueStruct_u64_ambiguous(output: *mut u8) -> FfiReturn;
        fn carbonate_AmbiguousY_OpaqueStruct_u32_ambiguous(output: *mut u8) -> FfiReturn;
    }

    unsafe {
        assert_eq!(
            FfiReturn::Ok,
            carbonate_OpaqueStruct_u32_ambiguous(output.as_mut_ptr())
        );
        let inherent = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::Inherent, inherent);

        assert_eq!(
            FfiReturn::Ok,
            carbonate_AmbiguousX_u32_OpaqueStruct_u32_ambiguous(output.as_mut_ptr())
        );
        let ambiguous_x = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

        assert_eq!(
            FfiReturn::Ok,
            carbonate_AmbiguousX_u64_OpaqueStruct_u64_ambiguous(output.as_mut_ptr())
        );
        let ambiguous_x = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

        assert_eq!(
            FfiReturn::Ok,
            carbonate_AmbiguousY_OpaqueStruct_u32_ambiguous(output.as_mut_ptr())
        );
        let ambiguous_y = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousY, ambiguous_y);
    }
}
