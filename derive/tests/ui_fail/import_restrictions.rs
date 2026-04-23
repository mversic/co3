use co3::{extern_, extern_C};

trait Kita {
    type U;

    fn kita(self);
}

extern_! {}

extern_C! {
    #![abi = "C"]
}

extern_! {
    #![abi = "Rust"]
    #![abi = "C"]
}

extern_C! {
    trait Kita {
        fn kita(self);
    }
}

extern_C! {
    enum Kita {}
}

extern_C! {
    struct Kita {}
}

extern_C! {
    union Kita {}
}

extern_C! {
    impl Kita for u32 {
        fn kita(self);
    }
}

extern_C! {
    #![link(crate = "kita")]

    impl Kita for u32 {
        #[dispatch]
        fn kita(self);
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch]
    impl<dyn(u32) T> Kita for Box<T> {
        fn kita(self, id: <dyn T>::ID);
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<u32>)]
    impl<dyn(u32) U, dyn(u8) T> Kita for (T, U) {
        fn kita(self, t_id: <dyn T>::ID, u_id: <dyn U>::ID);
    }
}

extern_! {
    #![abi = "C"]

    #[dispatch]
    impl Kita {
        fn kita() {}
    }
}

extern_C! {
    fn kita1(a: u32) {}
}

extern_C! {
    fn kita1((a, b): (u32, u32));
}

extern_C! {
    #![link(crate = "kita")]

    #[id(u8)]
    type Handle<T>;

    #[dispatch]
    impl<T> Drop for dyn Handle<T> {
        fn drop(self_id: <dyn Self>::ID, &mut self);
    }

    #[dispatch(<u8, i8>)]
    impl<dyn(u8) T> Clone for Handle<T> {
        fn clone(self_id: <dyn T>::ID, &self) -> Self;
    }
}

fn main() {}
