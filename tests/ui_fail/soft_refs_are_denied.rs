use co3::ffi;

mod provider {
    use co3::ffi;

    #[expect(unused_variables)]
    pub fn by_ref_is_allowed(arg1: &(u8,), arg2: Vec<(u8,)>) -> u8 {
        arg1.0
    }

    ffi! {
        #![unsafe(export("C"))]

        fn by_ref_is_allowed(#[soft] arg1: &(u8,), #[soft] arg2: Vec<(u8,)>) -> u8;
        fn by_val_is_denied(arg1: &(u8,), #[soft] arg2: move Vec<(u8,)>) -> u8;
    }

    #[expect(unused_variables)]
    pub fn by_val_is_denied(arg1: &(u8,), arg2: Vec<(u8,)>) -> u8 {
        arg1.0
    }
}

ffi! {
    #![unsafe(extern("C"))]

    pub extern "C" fn by_ref_is_allowed(#[soft] arg1: &(u8,), #[soft] arg2: Vec<(u8,)>) -> u8;
    pub extern "C" fn by_val_is_denied(arg1: &(u8,), #[soft] arg2: move Vec<(u8,)>) -> u8;
}

fn main() {}
