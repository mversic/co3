use co3::{Tag, ReprC, ffi, rust_spec::RustSpec};

#[derive(Clone, Debug, PartialEq, Eq, RustSpec, ReprC)]
#[repr(transparent)]
struct Value<T: ToOwned + ?Sized>(T::Owned);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct TransparentCTuple1<T: ?Sized>(T);

#[derive(Debug, Clone, Copy, RustSpec, Tag, ReprC)]
#[tag(u8, unsafe(1))]
#[repr(C)]
struct Opaque(u8);

impl Default for Opaque {
    fn default() -> Self {
        Self(3)
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    impl<dyn(u8) T: ToOwned = u8> Value<T>
    where
        use<T> @ <Opaque>,
    {
        move fn new(t_id: <dyn T>::TAG) -> Self;
    }

    impl<dyn(u8) T: ToOwned = u8> Value<T>
    where
        use<T> @ <Opaque>,
    {
        #[symbol_name = "ping"]
        fn ping2(
            t_id: <dyn T>::TAG,
            move self,
            #[soft] inc: &TransparentCTuple1<Opaque>,
        ) -> u8;
    }

    fn combine(move lhs: Value<u32>, #[soft] rhs: &(u8,)) -> u8;
}

mod provider {
    use core::ops::Add;

    use super::*;

    #[derive(Debug, Clone, Copy)]
    pub struct Opaque(u8);

    impl Add for Opaque {
        type Output = u8;

        fn add(self, rhs: Self) -> Self::Output {
            self.0 + rhs.0
        }
    }

    impl Default for Box<Opaque> {
        fn default() -> Self {
            Box::new(Opaque(3))
        }
    }

    impl<T: Add<Output = u8> + ToOwned<Owned = T>> Value<T>
    where
        Box<T>: Default,
    {
        fn new() -> Self {
            Self((*Box::<T>::default()).to_owned())
        }

        fn ping(self, inc: &TransparentCTuple1<T>) -> u8 {
            self.0 + inc.0.to_owned()
        }
    }

    fn combine(lhs: Value<u32>, rhs: &(u8,)) -> u8 {
        lhs.0 as u8 + rhs.0
    }

    ffi! {
        #![unsafe(export("C"))]

        #![symbol_prefix = "kita"]

        #[tag(u8, unsafe(1))]
        type Opaque;

        impl Default for Box<Opaque> {
            #[symbol_name = "kita__Default__OwnedOpaque__default"]
            move fn default() -> Self;
        }

        impl ToOwned for Box<Opaque> {
            move fn to_owned(&self) -> <Self as ToOwned>::Owned;
        }

        impl<dyn(u8) T: Add<Output = u8> + ToOwned<Owned = T> + ?Sized = u8> Value<T>
        where
            Box<T>: Default,
            use<T> @ <Opaque>,
        {
            move fn new() -> Self;
        }

        impl<
            dyn(u8) T: Add<Output = u8> + ToOwned<Owned = T> + ?Sized = u8,
        > Value<T>
        where
            Box<T>: Default,
            use<T> @ <Opaque>,
        {
            #[symbol_name = "ping"]
            fn ping(move self, #[soft] inc: &TransparentCTuple1<Opaque>) -> u8;
        }

        fn combine(move lhs: Value<u32>, #[soft] rhs: &(u8,)) -> u8;
    }
}

fn main() {
    let lhs = Value(4_u32);
    let rhs = (3u8,);
    assert_eq!(combine(lhs.clone(), &rhs), 7);

    let value = Value::new();
    let inc = Opaque::default();
    let inc: &Opaque = &inc;
    let inc = unsafe { &*(core::ptr::from_ref(inc).cast()) };
    assert_eq!(value.ping2(inc), 6);
}
