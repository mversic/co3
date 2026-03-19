use co3::{export_C, extern_C, handles};

struct Export1<T>(T);
struct Export2<T>(T);
struct Export3<T>(T);

handles! {
    Export2<u32>,
    Extern2<u32>,
}

export_C! {
    type OpaqueType<T>;
}

extern_C! {
    type ExternType<T>;
}

extern_C! {
    type ExternType;
}

export_C! {
    impl Drop for OpaqueType {
        fn drop(&mut self);
    }
}

extern_C! {
    #![link(crate = "kita")]

    impl Drop for ExternType {
        fn drop(&mut self);
    }
}

export_C! {
    type Export1<T>;

    impl Drop for Export1<u32> {
        fn drop(&mut self);
    }
}

extern_C! {
    #![link(crate = "kita")]

    type Extern1<T>;

    impl Drop for Extern1<u32> {
        fn drop(&mut self);
    }
}

export_C! {
    #[id(u32)]
    type Export2<T>;

    #[dispatch]
    impl Drop for Export2<u32> {
        fn drop(self_id: Self::ID, &mut dyn self);
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[id(u32)]
    type Extern2<T>;

    #[dispatch]
    impl Drop for Extern2<u32> {
        fn drop(self_id: Self::ID, &mut dyn self);
    }
}

fn main() {}
