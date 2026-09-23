use co3::ffi;

trait Counter: Sized {
    fn next(self) -> Self {
        self
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl Counter for u8 {
        raw fn next(self) -> u8;
    }
}

fn main() {}
