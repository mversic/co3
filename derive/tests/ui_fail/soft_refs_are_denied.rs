use co3::extern_C;

mod provider {
    use co3::{export, export_C};

    #[export("C")]
    pub extern "C" fn by_ref_is_allowed(
        #[soft] arg1: &(u8,),
        #[allow(unused_variables)]
        #[soft]
        arg2: Vec<(u8,)>,
    ) -> u8 {
        arg1.0
    }

    export_C! {
        pub extern "C" fn by_val_is_denied(arg1: &(u8,), #[soft] move arg2: Vec<(u8,)>) -> u8;
    }

    pub extern "C" fn by_val_is_denied(
        arg1: &(u8,),
        #[allow(unused_variables)] arg2: Vec<(u8,)>,
    ) -> u8 {
        arg1.0
    }
}

extern_C! {
    pub extern "C" fn by_ref_is_allowed(
        #[soft] arg1: &(u8,),
        #[soft] arg2: Vec<(u8,)>,
    ) -> u8;

    pub extern "C" fn by_val_is_denied(arg1: &(u8,), #[soft] move arg2: Vec<(u8,)>) -> u8;
}

fn main() {}
