use co3::raw;

struct Host;

impl Host {
    fn borrowed_generic<'a, T>(value: &'a T) -> &'a T {
        value
    }
}

raw! {
    impl Host {
        fn borrowed_generic<'a, T>(value: &'a T) -> &'a T;
    }
}

fn main() {}
