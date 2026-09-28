use co3::{Tag, ffi};

mod concrete {
    use super::ffi;

    struct Host;

    impl Host {
        const VALUE: u8 = 2;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Host {
            const VALUE: u8 = 1;
        }
    }
}

mod static_selection {
    use super::ffi;

    trait Value {
        const VALUE: u8;
    }

    struct Host<T>(core::marker::PhantomData<T>);

    impl Value for Host<u8> {
        const VALUE: u8 = 1;
    }

    impl Value for Host<u16> {
        const VALUE: u8 = 2;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl<T> Value for Host<T>
        where
            use<T> @ (<u8> | <u16>),
        {
            const VALUE: u8 = 1;
        }
    }
}

mod runtime_selection {
    use super::{Tag, ffi};

    #[derive(Tag)]
    #[tag(u8, unsafe(1))]
    struct First;

    #[derive(Tag)]
    #[tag(u8, unsafe(2))]
    struct Second;

    trait Value {
        const VALUE: u8;
    }

    struct Host<T>(core::marker::PhantomData<T>);

    impl Value for Host<First> {
        const VALUE: u8 = 1;
    }

    impl Value for Host<Second> {
        const VALUE: u8 = 2;
    }

    ffi! {
        #![unsafe(export("C"))]

        impl<dyn(u8) T> Value for Host<T>
        where
            use<T> @ (<First> | <Second>),
        {
            const VALUE: u8 = 1;
        }
    }
}

mod non_const_equality {
    use super::ffi;

    #[derive(PartialEq)]
    struct Value(u8);

    struct Host;

    impl Host {
        const VALUE: Value = Value(1);
    }

    ffi! {
        #![unsafe(export("C"))]

        impl Host {
            const VALUE: Value = Value(1);
        }
    }
}

mod const_generic_selection {
    use super::ffi;

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
            use<N> @ (<1> | <2>),
        {
            const VALUE: usize = 1;
        }
    }
}

fn main() {}
