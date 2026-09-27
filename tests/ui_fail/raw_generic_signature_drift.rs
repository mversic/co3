use co3::raw;

extern "system" fn source<T>(value: T) -> T {
    value
}

raw! {
    fn source<T>(value: move T) -> move T;
}

fn main() {}
