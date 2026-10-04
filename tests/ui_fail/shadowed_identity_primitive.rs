use co3::raw;

mod shadow {
    #[allow(non_camel_case_types)]
    pub type i32 = u64;
}

fn identity(value: shadow::i32) -> shadow::i32 {
    value
}

raw! {
    fn identity(value: shadow::i32) -> shadow::i32;
}

fn main() {}
