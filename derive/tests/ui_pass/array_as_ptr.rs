use co3::extern_C;

mod provider {
    use co3::{export, export_C};

    #[export("C", crate = "this")]
    pub fn array_arg(arr: [u32; 2]) -> [u32; 2] {
        arr
    }

    export_C! {
        #![export(crate = "this")]

        pub fn array_arg_move(move arr: [u32; 2]) -> [u32; 2];
    }

    pub fn array_arg_move(arr: [u32; 2]) -> [u32; 2] {
        array_arg(arr)
    }
}

extern_C! {
    #![link(crate = "this")]

    fn array_arg(arr: [u32; 2]) -> [u32; 2];
    // FIXME:
    //fn array_arg_move(move arr: [u32; 2]) -> [u32; 2];
}

fn main() {
    let arg = array_arg([1, 2]);
    assert_eq!([1, 2], arg);

    //let arg = array_arg_move([1, 2]);
    //assert_eq!([1, 2], arg);
}
