use co3::{export_C, extern_C, handles};

trait Kita {}

struct Export1<T>(T);

impl<T> Kita for Export1<T> {}

handles! {
    Export1<u32>,
}

export_C! {
    #[id(u8)]
    type OpaqueType<T>;
}

extern_C! {
    #[id(u8)]
    type ExternType<T>;
}

export_C! {
    #![export(crate = "kita")]

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

extern_C! {
    type ExternType;
}

extern_C! {
    #![link(crate = "kita")]

    #[id(u32)]
    type Extern1;

    #[dispatch(<Extern1>)]
    impl<dyn(u32) T> Drop for T {
        fn drop(&mut self, self_id: <dyn Self>::ID);
    }
}

export_C! {
    #[id(u32)]
    type Export1<T>;

    #[dispatch(<u32>)]
    // TODO: These Drop impls could be allowed
    impl<T> Drop for dyn Export1<T> where Self: Kita {
        fn drop(&mut self);
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[id(u32)]
    type Extern2<T>;

    #[dispatch]
    impl<T> Drop for dyn Extern2<T> where Self: Kita {
        fn drop(&mut self, self_id: <dyn Self>::ID);
    }
}

fn main() {}
