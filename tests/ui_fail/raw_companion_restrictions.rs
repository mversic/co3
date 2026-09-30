use co3::raw;

fn existing(value: u8) -> u8 {
    value
}

raw! {
    extern "C" fn existing(value: u8) -> u8;
}

fn local(value: u8) -> u8 {
    value
}

raw! {
    #[symbol_name = "local"]
    fn local(value: u8) -> u8;
}

fn borrowed(value: &str) {
    let _ = value;
}

raw! {
    fn borrowed<'a>(#[soft] value: &'a str);
}

fn callback_target(_: &u32) {}

raw! {
    fn callback_target(#[soft] value: &mut u32);
}

fn main() {}
