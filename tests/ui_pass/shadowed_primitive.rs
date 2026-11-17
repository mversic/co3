use co3::{ops::CFn1, raw};

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

fn main() {
    let identity: unsafe extern "C" fn(_) -> _ = identity_raw;
    let value = u64::MAX;
    let output: u64 = unsafe { identity.call(value) }.unwrap();
    assert_eq!(output, value);
}
