use core::marker::PhantomData;

use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Host<const N: usize>(u32, PhantomData<[u32; N]>);

impl<const N: usize> Host<N> {
    fn method(&self) {
        let _ = (self.0, N);
    }
}

fn free<const N: usize>() {
    let _ = N;
}

ffi! {
    #![unsafe(export("C"))]

    impl<const N: usize> Host<N>
    where
        use<N> @ (<1> | <2>),
    {
        #[symbol_name = "const_method_{N}"]
        fn method(&self);
    }

    #[symbol_name = "const_free_{N}"]
    fn free<const N: usize>()
    where
        use<N> @ (<1> | <2>);
}

mod imported {
    use super::*;

    #[derive(RustSpec, ReprC)]
    #[repr(transparent)]
    struct Host<const N: usize>(u32, PhantomData<[u32; N]>);

    ffi! {
        #![unsafe(extern("C"))]

        #[symbol_name = "const_free_{N}"]
        fn free<const N: usize>()
        where
            use<N> @ (<1> | <2>);

        impl<const N: usize> Host<N>
        where
            use<N> @ (<1> | <2>),
        {
            #[symbol_name = "const_method_{N}"]
            pub fn method(&self);
        }

        impl Host<1> {
            #[symbol_name = "const_method_param_{M}"]
            pub fn method_param<const M: usize>()
            where
                use<M> @ (<1> | <2>);
        }
    }

    pub(super) fn exercise() {
        free::<1>();
        Host::<1>(0, PhantomData).method();
    }
}

fn main() {
    imported::exercise();
}
