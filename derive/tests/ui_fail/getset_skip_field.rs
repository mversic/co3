use std::mem::MaybeUninit;

use co3::{FfiConvert, ExternC};
use getset::{Getters, Setters};

#[co3::carbonate]
#[derive(Clone, Setters, Getters, ExternC)]
#[getset(get = "pub")]
pub struct FfiStruct {
    #[getset(set = "pub")]
    a: i32,
    #[getset(skip)]
    b: u32,
}

fn main() {
    let s = FfiStruct { a: 42, b: 32 };

    let mut a = MaybeUninit::<*const i32>::uninit();
    let mut b = MaybeUninit::<*const u32>::uninit();

    unsafe {
        FfiStruct__a(FfiConvert::encode(&s, &mut ()), a.as_mut_ptr());
        let a: &i32 = FfiConvert::decode(a.assume_init(), &mut ()).unwrap();
        FfiStruct__set_a(
            FfiConvert::encode(&mut s, &mut ()),
            FfiConvert::encode(*a, &mut ()),
        );

        FfiStruct__b(FfiConvert::encode(&s, &mut ()), b.as_mut_ptr());
        let b: &u32 = FfiConvert::decode(b.assume_init(), &mut ()).unwrap();
        FfiStruct__set_b(
            FfiConvert::encode(&mut s, &mut ()),
            FfiConvert::encode(*b, &mut ()),
        );
    }
}
