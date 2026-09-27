use co3::raw;

trait Counter: Sized {
    fn next(self) -> Self {
        self
    }
}

raw! {
    impl Counter for u8 {
        fn next(self) -> u8;
    }
}

fn main() {}
