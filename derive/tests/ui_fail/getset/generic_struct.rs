use co3::{ExternC, export};
use getset::Getters;

#[export(extern "C")]
#[derive(Getters, ExternC)]
#[getset(get = "pub")]
pub struct FfiStruct<T> {
    inner: T,
}

fn main() {}
