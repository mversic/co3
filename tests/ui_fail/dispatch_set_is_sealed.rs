use co3::{Tag, ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
struct First(u8);

#[derive(RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(3))]
#[repr(transparent)]
struct Second(u8);

ffi! {
    #![unsafe(extern("C"))]

    impl First {
        fn sealed1<T>(value: move T)
        where
            use<T> @ <Self>;

        fn sealed2<T>(value: move T)
        where
            use<T> @ <Self>;
    }

    pub fn sealed1<T>(value: move T)
    where
        use<T> @ <First>;

    pub fn sealed2<T>(value: move T)
    where
        use<T> @ <First>;
}

impl first_sealed1::sealed::Sealed<Second> for () {}
impl first_sealed2::DispatchSet<Second> for () {
    fn sealed2(_: Second) {}
}

impl sealed1::sealed::Sealed<Second> for () {}
impl sealed2::DispatchSet<Second> for () {
    fn sealed2(_: Second) {}
}

fn main() {
    First::sealed1(Second(0));
    sealed1(Second(0));
}
