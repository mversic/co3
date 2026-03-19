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
    impl<T> Kita for Box<T> {
        fn kita(self);
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<u32>)]
    impl<U, T> Kita for (T, U) {
        fn kita(self);
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

    type Handle<T>;

    #[dispatch(<u32>)]
    impl<T> Drop for Handle<T> {
        fn drop(self_id: Self::ID, &mut dyn self);
    }

    #[dispatch(<u8, i8>)]
    impl<T> Clone for Handle<T> {
        fn clone(&self) -> Self;
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<'a>)]
    impl<'a> Kita<'a> {
        fn drop(&mut self);
    }
}

fn main() {}
