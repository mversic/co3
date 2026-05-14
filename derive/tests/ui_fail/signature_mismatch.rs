use co3::{export_C, extern_C, handles};

trait Kita {
    type MySelf;
    fn kita(&self) -> Self::MySelf;
    fn kita2(a: &u32);
}

handles! {
    Opaque1,
    Opaque2,
}

impl Kita for u32 {
    type MySelf = Box<Self>;

    fn kita(&self) -> Self::MySelf {
        unimplemented!()
    }

    fn kita2(_a: &u32) {}
}

fn kita(_a: &u32) {}

export_C! {
     fn kita(a: &Box<u32>);
}

export_C! {
    impl Kita for u32 {
        fn kita2(a: &Box<u32>);
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[id(u32)]
    type Opaque1;
    #[id(u8)]
    type Opaque2;

    impl ToOwned for Opaque1 {
        type Owned = OwnedOpaque1;

        fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    impl ToOwned for Opaque2 {
        type Owned = OwnedOpaque2;

        fn to_owned(&self) -> <Self as ToOwned>::Owned;
    }

    #[dispatch(<Opaque1>, <Opaque2>)]
    impl<dyn(u32) T: ToOwned> Kita for T {
        type MySelf = <T as ToOwned>::Owned;

        fn kita(self_id: <dyn Self>::ID, self: &Self) -> <Self as Kita>::MySelf;
        fn kita2(a: &u32, self_id: <dyn T>::ID);
    }
}

fn main() {}
