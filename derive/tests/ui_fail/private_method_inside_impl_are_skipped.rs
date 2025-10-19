use co3::{Encode, ExternC};

#[derive(Clone, ExternC)]
pub struct FfiStruct;

#[co3::carbonate]
impl FfiStruct {
    fn private(self) {}
}

fn main() {
    let s = FfiStruct;
    unsafe {
        FfiStruct__private(s.encode(&mut ()));
    }
}
