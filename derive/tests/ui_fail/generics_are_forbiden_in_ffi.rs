use getset::Getters;
use co3::{carbonate, FfiType};

#[carbonate]
pub fn freestanding<T>(v: T) -> T {
    v
}

#[carbonate]
#[derive(Getters, FfiType)]
#[getset(get = "pub")]
pub struct FfiStruct<T> {
    inner: T,
}

fn main() {}
