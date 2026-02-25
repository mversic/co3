use co3::{extern_, extern_C};

extern_! {}

extern_! {
    #![link(crate = "kita")]
}

extern_C! {
    #![abi = "C"]
}

extern_! {
    #![link_name = "kita"]
    #![abi = "kita"]
}

extern_! {
    #![abi = "kita"]

    #[link(name = "kita")]
    fn kita();
}

extern_C! {
    #![link_name = "kita"]
}

extern_! {
    #![abi = "C"]

    #[link_crate = "kita"]
    fn kita();
}

extern_C! {
    #[link_name = "kita"]
    impl Kita for u32 {}
}

trait Kita {}

fn main() {}
