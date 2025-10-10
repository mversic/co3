use std::mem::MaybeUninit;

use co3::{ExternC, FfiReturn, out_ptr::OutPtrRead as _};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ExternC)]
#[repr(u8)]
pub enum Ambiguous {
    Inherent,
    AmbiguousX,
    AmbiguousY,
    None,
}

#[derive(Clone, Copy, ExternC)]
pub struct FfiStruct;

#[co3::carbonate]
impl FfiStruct {
    pub fn ambiguous() -> Ambiguous {
        Ambiguous::Inherent
    }
}

pub trait AmbiguousX {
    fn ambiguous() -> Ambiguous;
}

pub trait AmbiguousY {
    fn ambiguous() -> Ambiguous;
}

#[co3::carbonate]
impl AmbiguousX for FfiStruct {
    fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousX
    }
}

#[co3::carbonate]
impl AmbiguousY for FfiStruct {
    fn ambiguous() -> Ambiguous {
        Ambiguous::AmbiguousY
    }
}

#[test]
fn unambiguous_method_call() {
    let mut output = MaybeUninit::new(Ambiguous::None as _);

    unsafe {
        assert_eq!(FfiReturn::Ok, FfiStruct__ambiguous(output.as_mut_ptr()));
        let inherent = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::Inherent, inherent);

        assert_eq!(
            FfiReturn::Ok,
            FfiStruct__AmbiguousX__ambiguous(output.as_mut_ptr())
        );
        let ambiguous_x = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousX, ambiguous_x);

        assert_eq!(
            FfiReturn::Ok,
            FfiStruct__AmbiguousY__ambiguous(output.as_mut_ptr())
        );
        let ambiguous_y = Ambiguous::try_read_out(output.assume_init()).unwrap();
        assert_eq!(Ambiguous::AmbiguousY, ambiguous_y);
    }
}
