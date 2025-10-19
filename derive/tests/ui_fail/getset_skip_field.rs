use std::mem::MaybeUninit;

use co3::{Encode, Decode, ExternC};
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
        FfiStruct__a((&s).encode(&mut ()), a.as_mut_ptr());
        let a: &i32 = Decode::decode(a.assume_init(), &mut ()).unwrap();
        FfiStruct__set_a(
            (&mut s).encode(&mut ()),
            *a.encode(&mut ()),
        );

        FfiStruct__b((&s).encode(&mut ()), b.as_mut_ptr());
        let b: &u32 = Decode::decode(b.assume_init(), &mut ()).unwrap();
        FfiStruct__set_b(
            (&mut s).encode(&mut ()),
            *b.encode(&mut ()),
        );
    }
}
