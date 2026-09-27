use co3::raw;

fn local(value: u8) -> u8 {
    value
}

raw! {
    #[symbol_name = "local"]
    fn local(value: u8) -> u8;
}

fn main() {}
