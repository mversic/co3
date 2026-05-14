use co3::{ReprC, export, extern_C, handles};

#[derive(Clone, Debug, PartialEq, Eq, ReprC)]
#[repr(transparent)]
struct Value<T: ?Sized>(Box<T>);

#[derive(ReprC)]
#[repr(transparent)]
struct TransparentCTuple1<T: ?Sized>(T);

handles! {
    Opaque,
}

extern_C! {
    #![link(crate = "kita")]

    #[id(u8)]
    pub type Opaque;

    impl Default for OwnedOpaque {
        fn default() -> Self;
    }

    impl ToOwned for Opaque {
        type Owned = OwnedOpaque;

        fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    #[dispatch(<Opaque>)]
    impl<dyn(u8) T: ToOwned + ?Sized> Value<T> {
        fn new(t_id: <dyn T>::ID) -> Self;

        #[link_name = "ping"]
        fn ping2(t_id: <dyn T>::ID, move self, #[soft] inc: &TransparentCTuple1<T>) -> u8;
    }

    fn combine(move lhs: Value<u32>, #[soft] rhs: &(u8,)) -> u8;
}

mod provider {
    use core::ops::Add;

    use co3::export_C;

    use super::*;

    handles! {
        Opaque,
    }

    // TODO: Should it be reported that `crate` is not supported on types?
    #[export("C", crate = "kita")]
    #[derive(Debug, Clone)]
    #[id(u8)]
    pub struct Opaque(u8);

    impl Add for Opaque {
        type Output = u8;

        fn add(self, rhs: Self) -> Self::Output {
            self.0 + rhs.0
        }
    }

    #[export("C", crate = "kita")]
    impl Default for Box<Opaque> {
        fn default() -> Self {
            Box::new(Opaque(3))
        }
    }

    export_C! {
        #![export(crate = "kita")]

        impl ToOwned for Box<Opaque> {
            fn to_owned(&self) -> <Self as ToOwned>::Owned;
        }
    }

    #[export("C", crate = "kita")]
    #[dispatch(<Opaque>)]
    impl<#[erased(u8)] T: Add<Output = u8> + ToOwned<Owned = T>> Value<T>
    where
        Box<T>: Default,
    {
        fn new() -> Self {
            Self(Box::default())
        }

        #[unsafe(export_name = "ping")]
        fn ping(#[by_val] self, #[soft] inc: &TransparentCTuple1<T>) -> u8 {
            *self.0 + inc.0.to_owned()
        }
    }

    // TODO: Support separate #[export(crate = "kita")]?
    #[export("C", crate = "kita")]
    fn combine(#[by_val] lhs: Value<u32>, #[soft] rhs: &(u8,)) -> u8 {
        (*lhs.0 as u8) + rhs.0
    }
}

fn main() {
    let lhs = Value(Box::new(4_u32));
    let rhs = (3u8,);
    assert_eq!(combine(lhs.clone(), &rhs), 7);

    let value = Value::new();
    let inc = OwnedOpaque::default();
    let inc: &Opaque = &inc;
    let inc = unsafe { &*(core::ptr::from_ref(inc).cast()) };
    assert_eq!(value.ping2(inc), 6);
}
