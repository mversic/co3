use co3::{ffi, rust_spec::RustSpec};

trait RefKita {
    fn kita(&self) -> u32;
}

#[derive(RustSpec)]
#[rust_spec(opaque)]
struct Exported0(u8);
impl RefKita for Exported0 {
    fn kita(&self) -> u32 {
        0
    }
}

#[derive(RustSpec)]
#[rust_spec(opaque)]
struct Exported1(u8);
impl RefKita for Exported1 {
    fn kita(&self) -> u32 {
        0
    }
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "dispatch_char_export"]

    #[tag(char, unsafe(0 as char))]
    type Exported0;

    impl<dyn(char) T> RefKita for T
    where
        use<T> @ <Exported0>,
    {
        fn kita(&self) -> u32;
    }
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "kita"]

    #[tag(char, unsafe(1 as char))]
    type Externed0;

    impl<dyn(char) T> RefKita for T
    where
        use<T> @ <Externed0>,
    {
        #[symbol_name = "kita"]
        fn kita(&self, self_id: <dyn Self>::TAG) -> u32;
    }
}

ffi! {
    #![unsafe(export("C"))]
    #![symbol_prefix = "dispatch_u32_export"]

    #[tag(u32, unsafe(0))]
    type Exported1;

    impl<dyn(u32) T> RefKita for T
    where
        use<T> @ (<Exported1> | <Exported1>),
    {
        fn kita(&self) -> u32;
    }
}

fn main() {}
