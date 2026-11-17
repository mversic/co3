use co3::raw;

fn independent<'a>(value: &(u8, bool), output: &'a u8) -> &'a u8 {
    assert!(value.1);
    output
}

raw! {
    fn independent<'a>(#[soft] value: &(u8, bool), output: &'a u8) -> &'a u8;
}

fn main() {
    let _ = independent_raw;
}
