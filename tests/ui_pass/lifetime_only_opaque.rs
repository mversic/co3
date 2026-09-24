use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "kita"]

    pub type Opaque<'a>;

    impl<'a> Default for OwnedOpaque<'a> {
        #[symbol_name = "kita__Default__Box_Opaque__default"]
        fn default() -> move Self;
    }

    impl Opaque<'_> {
        fn ping(&self);
    }
}

mod provider {
    use co3::ffi;

    struct Opaque<'a>(&'a u8);

    impl Default for Box<Opaque<'_>> {
        fn default() -> Self {
            Box::new(Opaque(&0))
        }
    }

    impl Opaque<'_> {
        fn ping(&self) {}
    }

    ffi! {
        #![unsafe(export("C"))]

        #![symbol_prefix = "kita"]

        pub type Opaque<'a>;

        // TODO: This should be allowed with '_ but it's not.
        // This is a special case where reference is materialized
        impl<'a> Default for Box<Opaque<'a>> {
            #[symbol_name = "kita__Default__Box_Opaque__default"]
            fn default() -> move Self;
        }

        impl Opaque<'_> {
            fn ping(&self);
        }
    }
}

fn main() {
    OwnedOpaque::default().ping();
}
