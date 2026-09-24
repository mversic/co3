use co3::ffi;

trait Kita {
    type MySelf;
    fn kita(&self) -> Vec<Self::MySelf>;
    fn kita2(a: &u32);
}

impl Kita for core::ffi::c_void {
    type MySelf = usize;

    fn kita(&self) -> Vec<Self::MySelf> {
        unreachable!()
    }

    fn kita2(_: &u32) {
        unreachable!()
    }
}

impl Kita for u32 {
    type MySelf = Self;

    fn kita(&self) -> Vec<Self::MySelf> {
        unimplemented!()
    }

    fn kita2(_a: &u32) {}
}

fn kita(_a: &u32) {}

ffi! {
    #![unsafe(export("C"))]

    fn kita(a: &Box<u32>);
}

ffi! {
    #![unsafe(export("C"))]

    impl Kita for u32 {
        fn kita2(a: &Box<u32>);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    #[tag(u32, unsafe(1))]
    type Opaque1;
    #[tag(u8, unsafe(2))]
    type Opaque2;

    impl ToOwned for Opaque1 {
        type Owned = OwnedOpaque1;

        fn to_owned(&self) -> move <Self as ToOwned>::Owned;
    }

    impl ToOwned for Opaque2 {
        type Owned = OwnedOpaque2;

        fn to_owned(&self) -> move <Self as ToOwned>::Owned;
    }

    impl<dyn(u32) T: ToOwned> Kita for T
    where
        use<T> @ (<Opaque1> | <Opaque2Alias>),
    {
        type MySelf = <T as ToOwned>::Owned;

        fn kita(self_id: <dyn Self>::TAG, self: &Self) -> move Vec<<Self as Kita>::MySelf>;
        fn kita2(a: &u32, self_id: <dyn T>::TAG);
    }
}

type Opaque2Alias = Opaque2;

fn main() {}
