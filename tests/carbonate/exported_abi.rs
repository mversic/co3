use std::mem::MaybeUninit;

use co3::{ExternC, FfiReturn, out_ptr::OutPtrRead as _};
use webassembly_test::webassembly_test;

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
pub struct OpaqueStruct;

trait AmbiguousX {
    fn ambiguous() -> Ambiguous;
}

trait AmbiguousY {
    fn ambiguous() -> Ambiguous;
}

#[co3::carbonate]
impl AmbiguousX for OpaqueStruct {
    fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

#[co3::carbonate]
impl AmbiguousY for OpaqueStruct {
    fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousY
    }
}

#[co3::carbonate]
impl OpaqueStruct {
    pub fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

#[test]
#[webassembly_test]
fn exported_abi() {
    let mut output = MaybeUninit::new(Ambiguous::None as _);

    unsafe extern "C" {
        fn OpaqueStruct__ambiguous(output: *mut u8) -> FfiReturn;
        fn OpaqueStruct__AmbiguousX__ambiguous(output: *mut u8) -> FfiReturn;
        fn OpaqueStruct__AmbiguousY__ambiguous(output: *mut u8) -> FfiReturn;
    }

    unsafe {
        assert_eq!(FfiReturn::Ok, OpaqueStruct__ambiguous(output.as_mut_ptr()));
        let inherent = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::Inherent, inherent);

        assert_eq!(
            FfiReturn::Ok,
            OpaqueStruct__AmbiguousX__ambiguous(output.as_mut_ptr())
        );
        let ambiguous_x = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

        assert_eq!(
            FfiReturn::Ok,
            OpaqueStruct__AmbiguousY__ambiguous(output.as_mut_ptr())
        );
        let ambiguous_y = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousY, ambiguous_y);
    }
}
