use co3::{unsafe_extern, unsafe_extern_C};

unsafe_extern! {}

unsafe_extern! {
    #![link(crate = "kita")]
}

unsafe_extern_C! {
    #![abi = "C"]
}

unsafe_extern! {
    #![link_name = "kita"]
    #![abi = "kita"]
}

unsafe_extern! {
    #![abi = "kita"]

    #[link(name = "kita")]
    fn kita();
}

unsafe_extern_C! {
    #![link_name = "kita"]
}

unsafe_extern! {
    #![abi = "C"]

    #[link_crate = "kita"]
    fn kita();
}

unsafe_extern_C! {
    #[link_name = "kita"]
    impl Kita for u32 {}
}

trait Kita {}

fn main() {}
