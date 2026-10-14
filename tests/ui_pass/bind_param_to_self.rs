use co3::{ffi, rust_spec::RustSpec};

trait Kita<U> {}

#[derive(RustSpec)]
#[rust_spec(opaque)]
struct UsedExport(u8);

impl Kita<Self> for UsedExport {}

impl UsedExport {
    fn kita(&self) {}
}

ffi! {
    #![unsafe(export("C"))]

    #[tag(u8, unsafe(8))]
    type UsedExport;

    impl<dyn(u8) U, V> Kita<U, V> for UsedExport
    where
        use<U> @ <Self>,
        use<V> @ <Self>,
    {}

    impl UsedExport {
        fn kita<dyn(u8) T, K>(self_: &K)
        where
            use<T> @ <Self>,
            use<K> @ <Self>;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8, unsafe(8))]
    type UsedExtern;

    impl<dyn(u8) U, V> Kita<U, V> for UsedExtern
    where
        use<U> @ <Self>,
        use<V> @ <Self>,
    {}

    impl UsedExtern {
        fn kita<dyn(u8) T, K>(self_: &K)
        where
            use<T> @ <Self>,
            use<K> @ <Self>;
    }
}

fn main() {}
