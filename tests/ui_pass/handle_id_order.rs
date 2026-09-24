use co3::{
    ffi,
    tag::{Tagged, TagFamily},
};

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "this_crate"]

    #[tag(u32, unsafe(1))]
    type Opaque1;
    #[tag(u8, unsafe(2))]
    #[derive(PartialEq)]
    type Opaque2;

    impl ToOwned for Opaque1 {
        type Owned = OwnedOpaque1;

        #[symbol_name = "this_crate__ToOwned__Box_Opaque1__to_owned"]
        fn to_owned(&self) -> move <Self as ToOwned>::Owned;
    }

    impl ToOwned for Opaque2 {
        type Owned = OwnedOpaque2;

        #[symbol_name = "this_crate__ToOwned__Box_Opaque2__to_owned"]
        fn to_owned(&self) -> move <Self as ToOwned>::Owned;
    }

    impl Default for OwnedOpaque1 {
        #[symbol_name = "this_crate__Default__Box_Opaque1__default"]
        fn default() -> move Self;
    }

    impl Default for OwnedOpaque2 {
        #[symbol_name = "this_crate__Default__Box_Opaque2__default"]
        fn default() -> move Self;
    }

    fn kita1(
        inc_id: <Opaque2 as TagFamily>::Kind,
        a_id: <Opaque1 as TagFamily>::Kind,
        a: &mut Opaque1,
        inc: &Opaque2,
    ) -> u8;
}

mod provider {
    use super::*;

    trait Custom<T> {
        fn kita1(&mut self, inc: &T) -> u8;
    }

    #[derive(Clone)]
    pub struct Opaque1(u8);
    #[derive(Clone)]
    pub struct Opaque2(u8);

    impl Default for Box<Opaque1> {
        fn default() -> Self {
            Box::new(Opaque1(0))
        }
    }

    impl Default for Opaque2 {
        fn default() -> Self {
            Self(0)
        }
    }

    impl<T> Custom<T> for Opaque1 {
        fn kita1(&mut self, _inc: &T) -> u8 {
            0
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #![symbol_prefix = "this_crate"]

        #[tag(u32, unsafe(1))]
        type Opaque1;
        #[tag(u8, unsafe(2))]
        type Opaque2;

        impl Default for Box<Opaque1> {
            #[symbol_name = "this_crate__Default__Box_Opaque1__default"]
            fn default() -> move Self;
        }

        impl Drop for Opaque2 {
            #[symbol_name = "this_crate__Drop__Opaque2__drop"]
            fn drop(&mut self);
        }

        impl Default for Box<Opaque2> {
            #[symbol_name = "this_crate__Default__Box_Opaque2__default"]
            fn default() -> move Self;
        }

        impl ToOwned for Box<Opaque1> {
            #[symbol_name = "this_crate__ToOwned__Box_Opaque1__to_owned"]
            fn to_owned(&self) -> move <Self as ToOwned>::Owned;
        }

        impl ToOwned for Box<Opaque2> {
            #[symbol_name = "this_crate__ToOwned__Box_Opaque2__to_owned"]
            fn to_owned(&self) -> move <Self as ToOwned>::Owned;
        }

        impl<dyn(u8) T, dyn(u32) U> Custom<T> for U
        where
            use<T, U> @ <Opaque2, Opaque1>,
        {
            #[symbol_name = "this_crate__kita1"]
            fn kita1(&mut self, inc: &T) -> u8;
        }
    }
}

fn main() {
    let mut value1 = OwnedOpaque1::default();
    let value2 = OwnedOpaque2::default();

    let _ = kita1(Opaque2::TAG, Opaque1::TAG, &mut value1, &value2);
}
