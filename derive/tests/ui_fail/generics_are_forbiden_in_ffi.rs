use co3::FfiType;
use getset::Getters;

#[co3::carbonate]
pub fn freestanding<T>(v: T) -> T {
    v
}

#[co3::carbonate]
#[derive(Getters, FfiType)]
#[getset(get = "pub")]
pub struct FfiStruct<T> {
    inner: T,
}

fn main() {}
