use co3::ffi;

trait Value {
    const VALUE: usize;
}

struct Host<const N: usize>;

impl<const N: usize> Value for Host<N> {
    const VALUE: usize = N;
}

ffi! {
    #![unsafe(export("C"))]

    impl<const N: usize> Value for Host<N>
    where
        use<N> @ <2>,
    {
        const VALUE: usize = N + 1;
    }
}

fn main() {}
