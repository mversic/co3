use std::mem::MaybeUninit;

use co3::{Decode, Encode, ExternC, ReprC, export};
use getset::{MutGetters, Setters};

#[export("C")]
#[derive(Clone, Setters, MutGetters, ReprC)]
pub struct FfiStruct {
    #[getset(set = "pub", get_mut = "pub")]
    a: u32,
    b: i32,
}

unsafe extern "C" {
    fn FfiStruct__a_mut(
        arg: <FfiStruct as ExternC>::CType,
        output: *mut <&mut u32 as ExternC>::CType,
    ) -> co3::FfiReturn;
    fn FfiStruct__set_a(
        arg: <FfiStruct as ExternC>::CType,
        value: <u32 as ExternC>::CType,
    ) -> co3::FfiReturn;
}

fn main() {
    let mut s = FfiStruct { a: 42, b: 32 };

    let mut a = MaybeUninit::<*mut u32>::uninit();
    let mut b = MaybeUninit::<*mut i32>::uninit();

    unsafe {
        FfiStruct__a_mut((&mut s).encode(&mut Default::default()), a.as_mut_ptr());
        let a: &mut u32 = Decode::decode(a.assume_init(), &mut Default::default()).unwrap();
        FfiStruct__set_a(
            (&mut s).encode(&mut Default::default()),
            *a.encode(&mut Default::default()),
        );

        FfiStruct__b_mut((&s).encode(&mut Default::default()), b.as_mut_ptr());
        let b: &mut i32 = Decode::decode(b.assume_init(), &mut Default::default()).unwrap();
        FfiStruct__set_b(
            (&mut s).encode(&mut Default::default()),
            *b.encode(&mut Default::default()),
        );
    }
}
