use co3::{ExternC, carbonate};
use getset::Getters;

#[carbonate(extern "C")]
#[derive(Getters, ExternC)]
#[getset(get = "pub")]
pub struct FfiStruct<T> {
    inner: T,
}

fn main() {}
