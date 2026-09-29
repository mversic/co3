use co3::raw;

fn selected<T>(_: T) {}

raw! {
    fn selected<T>(value: move T) where use<T> @ <u8>;
}

raw! {
    fn selected<dyn(u8) T = u8>(value: move T);
}

raw! {
    fn selected<dyn(u8) T = u8, U>(value: move T)
    where use<T> @ <u8>, use<U> @ <u8>;
}

fn main() {}
