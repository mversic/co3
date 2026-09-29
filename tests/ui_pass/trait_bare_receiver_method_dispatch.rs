use co3::{Tag, ffi};

#[derive(Tag)]
#[tag(u8, unsafe(1))]
struct Host;

trait Selected: co3::tag::Tagged<Kind = u8> + Sized {
    fn plain();
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(u8) H> Selected for H
    where
        use<H> @ <Host>,
    {
        fn plain();
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
    let _ = Host::inherent_first::<u8>;
    let _ = Host::inherent_second::<u16>;
}
