use co3::{ReprC, export_C, extern_C, external::Extern, handle::Id as HandleId, handles};

trait Custom<T> {
    fn kita1(&mut self, inc: &T) -> u8;
}

#[derive(ReprC)]
#[reprC(opaque)]
pub struct Opaque1;

#[derive(ReprC)]
#[reprC(opaque)]
pub struct Opaque2;

handles! {
    Opaque1,
    Opaque2,
}

impl<T> Custom<T> for Opaque1 {
    fn kita1(&mut self, _inc: &T) -> u8 {
        0
    }
}

export_C! {
    #[dispatch(
        Self = [Opaque1],
        T = [Opaque2],
    )]
    trait Custom<T> {
        #[id_pos(Self: 1, T: 3)]
        #[unsafe(export_name = "kita1")]
        fn kita1(&mut self, inc: &T) -> u8;
    }
}

extern_C! {
    #[link_name = "kita1"]
    fn kita1(
        a: *mut Extern,
        handle_id1: HandleId,
        inc: *const Extern,
        handle_id2: HandleId,
    ) -> u8;
}

fn main() {
    let mut value1 = Opaque1;
    let mut value2 = Opaque2;

    let ptr1 = core::ptr::from_mut(&mut value1);
    let ptr2 = core::ptr::from_ref(&mut value2);

    let _ = kita1(ptr1.cast::<Extern>(), 0, ptr2.cast::<Extern>(), 1);
}
