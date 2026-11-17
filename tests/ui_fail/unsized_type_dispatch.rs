use co3::{ReprC, Tag, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC, Tag)]
#[tag(u8)]
#[repr(transparent)]
struct Unsized<T: ?Sized>(T);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Wrapper<T: ?Sized>(Box<T>);

unsafe impl co3::tag::Tagged for Unsized<[u8]> {
    const TAG: u8 = 0;
}

mod provider {
    use super::*;

    impl Wrapper<Unsized<[u8]>> {
        fn take_export(self) -> usize {
            unimplemented!()
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        impl<dyn(u8) T = [u8]> Wrapper<T>
        where
            use<T> @ <Unsized<[u8]>>,
        {
            fn take_export(self: move Self) -> usize;
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    impl<dyn(u8) T = [u8]> Wrapper<T>
    where
        use<T> @ <Unsized<[u8]>>,
    {
        fn take(self: move Self, tag_id: <dyn T>::TAG) -> usize;
    }
}

fn main() {}
