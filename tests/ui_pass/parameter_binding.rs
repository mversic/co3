use core::marker::PhantomData;

use co3::{ReprC, Tag, ffi, rust_spec::RustSpec};

#[derive(Tag, RustSpec, ReprC)]
#[tag(u8, unsafe(1))]
#[repr(transparent)]
pub struct First(u8);

#[derive(Tag, RustSpec, ReprC)]
#[tag(u8, unsafe(2))]
#[repr(transparent)]
pub struct Second(u8);

#[derive(RustSpec, ReprC)]
#[repr(C)]
pub struct Host<T> {
    value: u8,
    _marker: PhantomData<T>,
}

ffi! {
    #![unsafe(extern("C"))]

    impl<T> Host<T>
    where
        use<T> @ <u8>,
    {
        fn first_bound();
    }

    impl<T> Host<T>
    where
        use<T> @ <u16>,
    {
        fn second();
    }

    impl<T> Host<T>
    where
        use<T> @ (<u8> | <u16>),
    {
        fn receiver(&self);

        fn shared();
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(u8) T> Host<T>
    where
        use<T> @ <First>,
    {
        #[symbol_name = "dynamic_first"]
        fn dynamic_first();
    }

    impl<dyn(u8) T> Host<T>
    where
        use<T> @ <Second>,
    {
        #[symbol_name = "dynamic_second"]
        fn dynamic_second();
    }

    impl<dyn(u8) T, U> Host<T>
    where
        use<T> @ <U>,
    {
        fn rhs_dependency<dyn(u8) K, L>()
        where
            use<K> @ <L>;
    }

    // FIXME: Should this be allowed
    //impl<dyn(u8) T, U> Host<T>
    //where
    //    use<U> @ (<First> | <Second>),
    //    use<T> @ <U>,
    //{
    //    #[symbol_name = "rhs_dependency__{U}"]
    //    fn rhs_dependency();
    //}
}

fn main() {}
