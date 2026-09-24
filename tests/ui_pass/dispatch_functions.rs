use co3::{Tag, ReprC, ffi, rust_spec::RustSpec};

trait ByteValue {
    fn byte(&self) -> u8;
}

#[derive(Clone, RustSpec, ReprC, Tag)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
struct First(u8);

#[derive(Clone, RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(2))]
#[repr(transparent)]
struct Second(u8);

impl ByteValue for (Second, Second) {
    fn byte(&self) -> u8 {
        self.0.0 + self.1.0
    }
}

impl ByteValue for (First, First) {
    fn byte(&self) -> u8 {
        self.0.0 + self.1.0
    }
}

impl ByteValue for First {
    fn byte(&self) -> u8 {
        self.0
    }
}

impl ByteValue for Second {
    fn byte(&self) -> u8 {
        self.0
    }
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct Host(u8);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct GenericHost<T>(core::marker::PhantomData<T>);

mod provider {
    use super::*;

    #[derive(RustSpec, ReprC)]
    #[repr(transparent)]
    struct ProviderHost(u8);

    #[derive(RustSpec, ReprC)]
    #[repr(transparent)]
    struct ProviderGenericHost<T>(core::marker::PhantomData<T>);

    fn dispatch_fn<T: ByteValue>(value: T) -> u8 {
        value.byte()
    }

    fn dispatch_echo<T: ByteValue>(value: T) -> T {
        value
    }

    fn dispatch_pair<T: ByteValue, U: ByteValue>(left: T, right: U) -> u8 {
        left.byte() + right.byte()
    }

    fn dispatch_product<T: ByteValue, U: ByteValue>(left: T, right: U) -> u8 {
        left.byte() + right.byte()
    }

    fn soft_dispatch<T>(value: &(T, T)) -> u8
    where
        (T, T): ByteValue,
    {
        value.byte()
    }

    impl ProviderHost {
        fn dispatch_method<T: ByteValue>(&self, value: T) -> u8 {
            self.0 + value.byte()
        }
    }

    impl<T: ByteValue> ProviderGenericHost<T> {
        fn dispatch_generic_method<U: ByteValue>(&self, value: U) -> u8 {
            let _ = self;
            value.byte()
        }
    }

    ffi! {
        #![unsafe(export("C"))]
        #![symbol_prefix = "function_dispatch"]

        fn dispatch_fn<dyn(u8) T: ByteValue = u8>(value: move T) -> u8
        where
            use<T> @ (<First> | <Second>);

        fn dispatch_echo<dyn(u8) T: ByteValue = u8>(value: move T) -> move T
        where
            use<T> @ (<First> | <Second>);

        fn dispatch_pair<dyn(u8) T: ByteValue = u8, dyn(u8) U: ByteValue = u8>(
            left: T,
            right: move U,
        ) -> u8
        where
            use<T, U> @ (<First, Second> | <Second, First>);

        fn dispatch_product<dyn(u8) T: ByteValue = u8, dyn(u8) U: ByteValue = u8>(
            left: T,
            right: U,
        ) -> u8
        where
            use<T> @ (<First> | <Second>),
            use<U> @ (<First> | <Second>);

        fn soft_dispatch<dyn(u8) T = u8>(#[soft] value: &(T, T)) -> u8
        where
            (T, T): ByteValue,
            use<T> @ (<First> | <Second>);

        impl ProviderHost {
            #[symbol_name = "function_dispatch_method"]
            fn dispatch_method<dyn(u8) T: ByteValue = u8>(&self, value: move T) -> u8
            where
                use<T> @ (<First> | <Second>);
        }

        impl<dyn(u8) T: ByteValue = u8> ProviderGenericHost<T>
        where
            use<T> @ (<First> | <Second>)
        {
            #[symbol_name = "function_dispatch_generic_method"]
            fn dispatch_generic_method<dyn(u8) U: ByteValue = u8>(&self, value: move U) -> u8
            where
                use<U> @ (<First> | <Second>);
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "function_dispatch"]

    pub fn dispatch_fn<dyn(u8) T: ByteValue = u8>(value: move T) -> u8
    where
        use<T> @ (<First> | <Second>);

    pub fn dispatch_echo<dyn(u8) T: ByteValue = u8>(value: move T) -> move T
    where
        use<T> @ (<First> | <Second>);

    pub fn dispatch_pair<dyn(u8) T: ByteValue = u8, dyn(u8) U: ByteValue = u8>(
        left: T,
        right: move U,
    ) -> u8
    where
        use<T, U> @ (<First, Second> | <Second, First>);

    pub fn dispatch_product<dyn(u8) T: ByteValue = u8, dyn(u8) U: ByteValue = u8>(
        left: T,
        right: U,
    ) -> u8
    where
        use<T> @ (<First> | <Second>),
        use<U> @ (<First> | <Second>);

    pub fn soft_dispatch<dyn(u8) T = u8>(#[soft] value: &(T, T)) -> u8
    where
        (T, T): ByteValue,
        use<T> @ (<First> | <Second>);

    impl Host {
        #[symbol_name = "function_dispatch_method"]
        pub fn dispatch_method<dyn(u8) T: ByteValue = u8>(&self, value: move T) -> u8
        where
            use<T> @ (<First> | <Second>);
    }

    impl<dyn(u8) T: ByteValue = u8> GenericHost<T>
    where
        use<T> @ (<First> | <Second>)
    {
        #[symbol_name = "function_dispatch_generic_method"]
        pub fn dispatch_generic_method<dyn(u8) U: ByteValue = u8>(&self, value: move U) -> u8
        where
            use<U> @ (<First> | <Second>);
    }
}

fn main() {
    assert_eq!(dispatch_fn(First(3)), 3);
    assert_eq!(dispatch_fn(Second(4)), 4);
    assert_eq!(dispatch_echo(First(9)).byte(), 9);
    assert_eq!(dispatch_echo(Second(10)).byte(), 10);
    assert_eq!(dispatch_pair(First(3), Second(4)), 7);
    assert_eq!(dispatch_pair(Second(5), First(6)), 11);
    assert_eq!(dispatch_product(First(3), Second(4)), 7);
    assert_eq!(dispatch_product(Second(5), First(6)), 11);
    assert_eq!(soft_dispatch(&(First(12), First(12))), 24);
    assert_eq!(Host(5).dispatch_method(First(6)), 11);
    assert_eq!(Host(7).dispatch_method(Second(8)), 15);
    assert_eq!(
        GenericHost::<First>(core::marker::PhantomData).dispatch_generic_method(Second(6)),
        6,
    );
}
