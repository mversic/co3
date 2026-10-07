use co3::{tag::Tagged, ffi};

trait Kita {}

struct Export1<T>(T);

impl<T> Kita for Export1<T> {}

unsafe impl Tagged for Export1<u32> {
    const TAG: Self::Kind = 0;
}

ffi! {
    #![unsafe(export("C"))]

    type ReturningDrop;

    impl Drop for ReturningDrop {
        fn drop(&mut self) -> i16;
    }
}

ffi! {
    #![unsafe(export("C"))]

    #[tag(u8)]
    type First;

    #[tag(u8)]
    type Second;

    impl<T> Drop for T
    where
        use<T> @ (<First> | <Second>),
    {
        fn drop(&mut self);
    }

    impl<T> Drop for T
    where
        use<T> @ (<First> | <Second>),
    {
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8)]
    type First;

    #[tag(u8)]
    type Second;

    impl<T> Drop for T
    where
        use<T> @ (<First> | <Second>),
    {
        fn drop(&mut self);
    }

    impl<T> Drop for T
    where
        use<T> @ (<First> | <Second>),
    {
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(export("C"))]

    type Opaque<T>;

    impl<dyn(u8) T> Drop for Opaque<T>
    where
        use<T> @ <u8>,
    {
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(export("C"))]

    #[tag(u32)]
    type Export1<T>;

    // TODO: These Drop impls could be allowed
    impl<T> Drop for dyn Export1<T>
    where
        Self: Kita,
        use<T> @ <u32>,
    {
        fn drop(&mut self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    #[tag(u32)]
    type Extern2<T>;

    impl<T> Drop for dyn Extern2<T>
    where
        Self: Kita,
        use<T> @ (<u32>),
    {
        fn drop(&mut self, self_id: <dyn Self>::TAG);
    }
}

fn main() {}
