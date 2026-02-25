use co3::{ReprC, export};
use getset::Getters;

#[export("C")]
#[derive(Getters, ReprC)]
#[getset(get = "pub")]
pub struct FfiStruct<T> {
    inner: T,
}

fn main() {}
