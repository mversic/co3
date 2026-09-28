use co3::{Tag, ffi};

#[derive(Tag)]
#[tag(u8, unsafe(1))]
struct Host;

trait Selected: co3::tag::Tagged<Kind = u8> + Sized {
    fn plain();

    fn first<T>()
    where
        (): h_first::DispatchSet<Self, T>;

    fn second<T>()
    where
        (): h_second::DispatchSet<Self, T>;
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(u8) H> Selected for H
    where
        use<H> @ <Host>,
    {
        fn plain();

        #[symbol_name = "first_{T}"]
        fn first<T>()
        where
            use<T> @ <u8>;

        #[symbol_name = "second_{T}"]
        fn second<T>()
        where
            use<T> @ <u16>;
    }

    impl<dyn(u8) H> H
    where
        use<H> @ <Host>,
    {
        fn inherent_plain();

        #[symbol_name = "inherent_first_{T}"]
        fn inherent_first<T>()
        where
            use<T> @ <u8>;

        #[symbol_name = "inherent_second_{T}"]
        fn inherent_second<T>()
        where
            use<T> @ <u16>;
    }
}

fn main() {
    let _ = <Host as Selected>::first::<u8>;
    let _ = <Host as Selected>::second::<u16>;
}
