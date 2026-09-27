use co3::raw;

raw! {
    type Ordinary = u8;
}

raw! {
    type Callback = raw extern "C" fn(u8);
}

fn main() {}
