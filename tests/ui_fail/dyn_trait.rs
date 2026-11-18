use co3::ffi;

trait Trait {
    fn value(&self) -> u8;
}

ffi! {
    #![unsafe(export("C"))]

    impl dyn Trait {
        fn value(&self) -> u8;
    }
}

fn main() {}
