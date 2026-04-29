use co3::{ReprC, export, extern_C, handles};

#[derive(Clone, Debug, PartialEq, Eq, ReprC)]
#[repr(C)]
struct Value<T>(Box<T>);

handles! {
    Opaque,
}

extern_C! {
    #![link(crate = "kita")]

    #[id(u8)]
    pub type Opaque;

    impl Default for Opaque {
        fn default() -> Self;
    }

    impl Clone for Opaque {
        fn clone(&self) -> Self;
    }

    #[dispatch(<Opaque>)]
    impl<dyn(u8) T> Value<T> {
        #[link_name = "ping"]
        fn ping2(t_id: <dyn T>::ID, move self, #[unstable_refs] inc: &(T,)) -> u8;

        fn new(t_id: <dyn T>::ID) -> Self;
    }

    fn combine(move lhs: Value<u32>, #[unstable_refs] rhs: &(u8,)) -> u8;
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
    #[id(u8)]
    #[derive(Debug)]
    pub struct Opaque(u8);

    impl Clone for Opaque {
        fn clone(&self) -> Self {
            Self::default()
        }
    }

    impl Add for Opaque {
        type Output = u8;

        fn add(self, rhs: Self) -> Self::Output {
            self.0 + rhs.0
        }
    }

    #[export("C", crate = "kita")]
    impl Default for Opaque {
        fn default() -> Self {
            Self(3)
        }
    }

    export_C! {
        #![export(crate = "kita")]

        impl Clone for Opaque {
            fn clone(&self) -> Self;
        }
    }

    #[export("C", crate = "kita")]
    #[dispatch(<Opaque>)]
    impl<#[erased(u8)] T: Add<Output = u8> + Clone + Default> Value<T> {
        #[unsafe(export_name = "ping")]
        fn ping(#[by_val] self, #[unstable_refs] inc: &(T,)) -> u8 {
            *self.0 + inc.0.clone()
        }

        fn new() -> Self {
            Self(Box::default())
        }
    }

    // TODO: Support separate #[export(crate = "kita")]?
    #[export("C", crate = "kita")]
    fn combine(#[by_val] lhs: Value<u32>, #[unstable_refs] rhs: &(u8,)) -> u8 {
        (*lhs.0 as u8) + rhs.0
    }
}

fn main() {
    let lhs = Value(Box::new(4_u32));
    let rhs = (3u8,);
    assert_eq!(combine(lhs.clone(), &rhs), 7);

    let value = Value::new();
    let inc = Opaque::default();
    assert_eq!(value.ping2(&(inc,)), 6);
}
