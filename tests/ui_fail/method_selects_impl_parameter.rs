use co3::ffi;

struct Host<const N: usize>;

trait Service {
    fn first();
    fn second();
}

ffi! {
    #![unsafe(extern("C"))]

    impl<const N: usize> Service for Host<N> {
        fn first()
        where
            use<N> @ <1>;

        fn second()
        where
            use<N> @ <2>;
    }

    impl<const N: usize> Host<N> {
        fn inherent_first()
        where
            use<N> @ <1>;

        fn inherent_second()
        where
            use<N> @ <2>;
    }
}

fn main() {}
