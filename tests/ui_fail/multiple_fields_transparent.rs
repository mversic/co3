use co3::ReprC;

#[derive(ReprC)]
#[repr(transparent)]
enum MultiVariantEnum<T> {
    A { itm: T },
    B,
}

#[derive(ReprC)]
#[repr(transparent)]
enum MultiFieldEnum<T> {
    A(T, T),
}

#[derive(ReprC)]
#[repr(transparent)]
struct MultiFieldStruct<T>(T, T);

fn main() {}
