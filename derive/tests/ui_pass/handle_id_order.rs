use core::ptr::NonNull;

use co3::{
    extern_C,
    handle::{Handle, HandleFamily},
    handles,
};

handles! {
    Opaque1,
    Opaque2,
}

extern_C! {
    #![link(crate = "this_crate")]

    #[id(u32)]
    type Opaque1;
    #[id(u8)]
    type Opaque2;

    fn kita1(
        inc_id: <Opaque2 as HandleFamily>::Kind,
        a_id: <Opaque1 as co3::handle::HandleFamily>::Kind,
        a: &mut Opaque1,
        inc: &Opaque2,
    ) -> u8;
}

mod provider {
    use co3::{export, export_C, handles};

    trait Custom<T> {
        fn kita1(&mut self, inc: &T) -> u8;
    }

    #[export("C", crate = "this_crate")]
    #[id(u32)]
    pub struct Opaque1;
    pub struct Opaque2;

    handles! {
        Opaque1,
        Opaque2,
    }

    impl<T: co3::handle::Handle> Custom<T> for Opaque1 {
        fn kita1(&mut self, _inc: &T) -> u8 {
            0
        }
    }

    export_C! {
        #[id(u8)]
        type Opaque2;

        impl Drop for Opaque2 {
            #[unsafe(export_name = "this_crate__Drop__Opaque2__drop")]
            fn drop(&mut self);
        }

        #[dispatch(<Opaque2, Opaque1>)]
        impl<dyn(u8) T, dyn(u32) U> Custom<T> for U {
            #[unsafe(export_name = "this_crate__kita1")]
            fn kita1(&mut self, inc: &T) -> u8;
        }
    }
}

fn main() {
    let mut value1 = Opaque1(NonNull::dangling());
    let value2 = Opaque2(NonNull::dangling());

    let _ = kita1(Opaque2::ID, Opaque1::ID, &mut value1, &value2);
}
