use co3::{ReprC, extern_, extern_C};

trait Kita {
    type U;

    fn kita(self);
}

#[derive(ReprC)]
#[reprC(opaque)]
enum Opaque {}

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
    impl Kita for u32 {
        type U = u32;
    }
}

extern_C! {
    impl Kita for i32 {}
}

extern_C! {
    impl Kita for i32 {
        fn kita(self);
    }
}

extern_C! {
    #[link_name]
    type Opaque;
}

extern_C! {
    type Handle;
}

extern_C! {
    type Handle;

    #[dispatch(Self = [Opaque])]
    impl<T> Drop for T {
        fn drop(&mut self);
    }
}

fn main() {}
