use co3::raw;

fn process(value: (u8, u8)) {
    let _ = value;
}

raw! {
    fn process(#[unpack(u8, u8)] value: (u8, u8));
}

fn main() {}
