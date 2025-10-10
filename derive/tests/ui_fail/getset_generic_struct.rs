use co3::ExternC;
use getset::Getters;

#[co3::carbonate]
#[derive(Getters, ExternC)]
#[getset(get = "pub")]
pub struct FfiStruct<T> {
    inner: T,
}

fn main() {}
