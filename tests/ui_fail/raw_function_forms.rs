use co3::raw;

struct Value;

raw! {
    raw fn existing();
}

raw! {
    impl Value {
        raw fn method(self);
    }
}

raw! {
    unsafe extern "system" fn existing();
}

fn main() {}
