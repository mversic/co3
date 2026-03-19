use co3::extern_C;

mod provider {
    use co3::export;

    #[export("C", crate = "this")]
    pub fn array_arg(arr: [u32; 2]) -> [u32; 2] {
        arr
    }
}

extern_C! {
    #![link(crate = "this")]

    fn array_arg(arr: [u32; 2]) -> [u32; 2];
}

fn main() {
    let arg = array_arg([1, 2]);
    assert_eq!([1, 2], arg);
}
