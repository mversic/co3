use co3::raw;

fn existing(value: u8) -> u8 {
    value
}

raw! {
    extern "C" fn existing(value: u8) -> u8;
}

fn main() {}
