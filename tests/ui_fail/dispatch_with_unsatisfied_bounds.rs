use co3::{ffi, rust_spec::RustSpec, tag::Tagged};

pub trait Unimplemented {}

trait RefKita {
    fn kita(&self) -> u32;
}

#[derive(RustSpec)]
#[rust_spec(opaque)]
struct Exported2<T>(T);
impl RefKita for Exported2<u32> {
    fn kita(&self) -> u32 {
        unimplemented!()
    }
}

unsafe impl<T> Tagged for Exported2<T> {
    const TAG: Self::Kind = 0;
}

ffi! {
    #![unsafe(export("C"))]

    #[tag(u8)]
    type Exported2<T>;

    impl<T> Drop for dyn Exported2<T>
    where
        use<T> @ <u32>,
    {
        fn drop(&mut self);
    }

    impl<dyn(u8) T: Unimplemented> RefKita for T
    where
        i32: Unimplemented,
        use<T> @ <Exported2<u32>>,
    {
        fn kita(&self) -> u32;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u64, unsafe(1))]
    type Externed2<T>;

    impl<T> Drop for dyn Externed2<T>
    where
        use<T> @ <u32>,
    {
        #[symbol_name = "drop"]
        fn drop(&mut self, self_id: <dyn Externed2<T>>::TAG);
    }

    impl<dyn(u64) T: Unimplemented> RefKita for T
    where
        i32: Unimplemented,
        use<T> @ <Externed2<u32>>,
    {
        #[symbol_name = "kita"]
        fn kita(&self, self_id: <dyn Self>::TAG) -> u32;
    }
}

fn main() {}
