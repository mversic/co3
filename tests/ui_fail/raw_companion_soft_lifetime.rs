use co3::raw;

fn borrowed(value: &str) {
    let _ = value;
}

raw! {
    fn borrowed<'a>(#[soft] value: &'a str);
}

fn main() {}
