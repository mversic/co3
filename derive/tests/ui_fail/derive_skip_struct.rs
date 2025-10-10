use std::mem::MaybeUninit;

use getset::{MutGetters, Setters};
use co3::{FfiConvert, ExternC};

#[co3::carbonate]
#[derive(Clone, Setters, MutGetters, ExternC)]
// TODO: I am not really sure what is the purpose of this test
// getset allows `#[getset(skip)]` to be placed on a struct, but it doesn't seem to have any effect at all
// Due to it being potentially error-prone, co3_derive disallows such placement hence it's commented out here
// #[getset(skip)]
pub struct FfiStruct {
    #[getset(set = "pub", get_mut = "pub")]
    a: u32,
    b: i32,
}

fn main() {
    let s = FfiStruct { a: 42, b: 32 };

    let mut a = MaybeUninit::<*mut u32>::uninit();
    let mut b = MaybeUninit::<*mut i32>::uninit();

    unsafe {
        FfiStruct__a_mut(FfiConvert::encode(&mut s, &mut ()), a.as_mut_ptr());
        let a: &mut u32 = FfiConvert::decode(a.assume_init(), &mut ()).unwrap();
        FfiStruct__set_a(
            FfiConvert::encode(&mut s, &mut ()),
            FfiConvert::encode(*a, &mut ()),
        );

        FfiStruct__b_mut(FfiConvert::encode(&s, &mut ()), b.as_mut_ptr());
        let b: &mut i32 = FfiConvert::decode(b.assume_init(), &mut ()).unwrap();
        FfiStruct__set_b(
            FfiConvert::encode(&mut s, &mut ()),
            FfiConvert::encode(*b, &mut ()),
        );
    }
}
