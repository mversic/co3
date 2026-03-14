use co3::{ReprC, extern_, extern_C};

trait Kita {
    type U;

    fn kita(self);
}

#[derive(ReprC)]
#[reprC(opaque)]
enum Opaque {}

extern_! {}

extern_C! {
    #![abi = "C"]
}

extern_! {
    #![abi = "kita"]

    #[link(name = "kita")]
    fn kita();
}

extern_! {
    #![abi = "C"]

    #[link(crate = "kita")]
    fn kita();
}

extern_C! {
    #[link_name = "kita"]
    impl Kita for u32 {
        type U = u32;

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
    impl Kita for u32 {
        #[dispatch]
        fn kita(self);
    }
}

extern_C! {
    type Handle;

    #[dispatch()]
    impl<T> Drop for T {
        fn drop(&mut self);
    }
}

extern_C! {
    type Handle<T>;

    #[dispatch]
    impl<T> Drop for Handle<T> {
        fn drop(&mut self, self_id: Self::Id);
    }

    #[dispatch(
        T = [Handle<u8>],
        T = [Handle<i8>],
    )]
    impl<T> Clone for Handle<T> {
        fn clone(&self) -> Self;
    }
}

extern_C! {
    trait Kita {
        fn kita(self);
    }
}

extern_C! {
    #[dispatch()]
    impl<T> Kita for Box<T> {
        fn kita(self);
    }
}

extern_C! {
    #[dispatch(
        T = [u32],
        U = [u8],
    )]
    impl<U> Kita for U {
        fn kita(self);
    }
}

fn main() {}
