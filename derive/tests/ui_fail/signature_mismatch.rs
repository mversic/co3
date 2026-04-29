use co3::{export_C, extern_C, handles};

trait Kita {
    fn kita(self) -> Self;
    fn kita2(a: &u32);
}

handles! {
    Opaque1,
    Opaque2,
}

impl Kita for u32 {
    fn kita(self) -> Self {
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

    #[dispatch(<Opaque1>, <Opaque2>)]
    impl<dyn(u32) T> Kita for T {
        fn kita(self_id: <dyn Self>::ID, self: Self) -> Self;
        fn kita2(a: &u32, self_id: <dyn T>::ID);
    }
}

fn main() {}
