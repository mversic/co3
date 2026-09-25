use core::{borrow::Borrow, ffi::c_void};

use co3::{ReprC, Tag, CType, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC, Tag)]
#[tag(u8)]
#[repr(transparent)]
struct Unsized<T: ?Sized>(T);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Wrapper<T: ?Sized>(Box<T>);

unsafe impl co3::tag::Tagged for Unsized<str> {
    const TAG: u8 = 0;
}

impl From<Box<Unsized<str>>> for Unsized<String> {
    fn from(_value: Box<Unsized<str>>) -> Self {
        unimplemented!()
    }
}

impl Borrow<Unsized<str>> for Unsized<String> {
    fn borrow(&self) -> &Unsized<str> {
        unimplemented!()
    }
}

impl ToOwned for Unsized<str> {
    type Owned = Unsized<String>;

    fn to_owned(&self) -> Self::Owned {
        unimplemented!()
    }
}

impl Clone for Wrapper<Unsized<str>> {
    fn clone(&self) -> Self {
        unimplemented!()
    }
}

mod provider {
    use super::*;

    impl From<Unsized<String>> for Box<Unsized<str>> {
        fn from(_: Unsized<String>) -> Self {
            unimplemented!()
        }
    }

    impl Wrapper<Unsized<str>> {
        fn take_export(self) -> usize {
            unimplemented!()
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        impl<dyn(u8) T = [c_void]> Wrapper<T>
        where
            use<T> @ <Unsized<str>>,
        {
            fn take_export(self) -> usize;
        }
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #![symbol_prefix = "kita"]

    impl<dyn(u8) T = [c_void]> Wrapper<T>
    where
        use<T> @ <Unsized<str>>,
    {
        fn take(self, tag_id: <dyn T>::TAG) -> usize;
    }
}

fn main() {}
