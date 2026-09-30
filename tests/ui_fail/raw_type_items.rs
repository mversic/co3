use co3::raw;

struct Host;

fn existing() {}

raw! {
    type Opaque;
}

raw! {
    struct Value;
}

raw! {
    enum Choice { A }
}

raw! {
    type Ordinary = u8;
}

raw! {
    type Callback = raw extern "C" fn(u8);
}

raw! {
    raw fn existing();
}

raw! {
    impl Host {
        raw fn method(self);
    }
}

raw! {
    unsafe extern "system" fn existing();
}

fn main() {}
