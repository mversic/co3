use co3::extern_C;

extern_C! {
    #![link(crate = "kita")]

    pub type Opaque<'a>;

    impl<'a> Default for OwnedOpaque<'a> {
        #[link_name = "kita__Default__Box_Opaque__default"]
        fn default() -> Self;
    }

    impl<'a> Opaque<'a> {
        fn ping(&self);
    }
}

mod provider {
    use co3::export;

    #[export("C", crate = "kita")]
    struct Opaque<'a>(&'a u8);

    #[export("C", crate = "kita")]
    impl<'a> Default for Box<Opaque<'a>> {
        #[unsafe(export_name = "kita__Default__Box_Opaque__default")]
        fn default() -> Self {
            Box::new(Opaque(&0))
        }
    }

    #[export("C", crate = "kita")]
    impl<'a> Opaque<'a> {
        fn ping(&self) {}
    }
}

fn main() {
    OwnedOpaque::default().ping();
}
