use co3::{borrow::CBorrowed, raw};

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
    let identity: unsafe extern "C" fn(CBorrowed<u64>) -> CBorrowed<u64> = identity_raw;
    let value = u64::MAX;
    assert_eq!(
        unsafe { identity(CBorrowed::new(value)) }.into_inner(),
        value
    );
}
