use co3::ffi;

struct Host;

trait Service {
    fn lifetime<'a>(&'a self);
    fn typed<T>(&self);
    fn konst<const N: usize>(&self);
}

ffi! {
    #![unsafe(extern("C"))]

    impl Service for Host {
        fn lifetime<'a>(&'a self);
        fn typed<T>(&self);
        fn konst<const N: usize>(&self);
    }
}

fn main() {}
