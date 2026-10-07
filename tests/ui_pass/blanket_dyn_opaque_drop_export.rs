use co3::ffi;

struct First(u8);
struct Second(u8);

impl Drop for First {
    fn drop(&mut self) {}
}

impl Drop for Second {
    fn drop(&mut self) {}
}

ffi! {
    #![unsafe(export("C"))]

    #[tag(u8, unsafe(1))]
    type First;

    #[tag(u8, unsafe(2))]
    type Second;

    impl<dyn(u8) T> Drop for T
    where
        use<T> @ (<First> | <Second>),
    {
        fn drop(&mut self);
    }
}

fn main() {}
