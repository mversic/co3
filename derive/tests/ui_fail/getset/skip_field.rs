use std::mem::MaybeUninit;

use co3::{Decode, Encode, ExternC, ReprC, export};
use getset::{Getters, Setters};

#[export("C")]
#[derive(Clone, Setters, Getters, ReprC)]
#[getset(get = "pub")]
pub struct FfiStruct {
    #[getset(set = "pub")]
    a: i32,
    #[getset(skip)]
    b: u32,
}

unsafe extern "C" {
    fn FfiStruct__a(
        arg: <FfiStruct as ExternC>::CType,
        output: *mut <&i32 as ExternC>::CType,
    ) -> co3::FfiReturn;
    fn FfiStruct__set_a(
        arg: <FfiStruct as ExternC>::CType,
        value: <i32 as ExternC>::CType,
    ) -> co3::FfiReturn;
}

fn main() {
    let mut s = FfiStruct { a: 42, b: 32 };

    let mut a = MaybeUninit::<*const i32>::uninit();
    let mut b = MaybeUninit::<*const u32>::uninit();

    unsafe {
        FfiStruct__a((&s).encode(&mut Default::default()), a.as_mut_ptr());
        let a: &i32 = Decode::decode(a.assume_init(), &mut Default::default()).unwrap();
        FfiStruct__set_a(
            (&mut s).encode(&mut Default::default()),
            *a.encode(&mut ()),
        );

        FfiStruct__b((&s).encode(&mut Default::default()), b.as_mut_ptr());
        let b: &u32 = Decode::decode(b.assume_init(), &mut Default::default()).unwrap();
        FfiStruct__set_b(
            (&mut s).encode(&mut Default::default()),
            *b.encode(&mut Default::default()),
        );
    }
}
