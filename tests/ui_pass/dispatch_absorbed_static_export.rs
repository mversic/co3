use core::marker::PhantomData;

use co3::{ffi, rust_spec::RustSpec};

trait Mixed<T> {
    fn hidden(&self);
    fn exposed(&self, value: &T);
}

#[derive(RustSpec)]
#[rust_spec(opaque)]
struct Resource<T>(u8, PhantomData<T>);

impl<T> Drop for Resource<T> {
    fn drop(&mut self) {}
}

impl<T> Mixed<T> for Resource<T> {
    fn hidden(&self) {
        let _ = self.0;
    }

    fn exposed(&self, value: &T) {
        let _ = (self.0, value);
    }
}

unsafe impl co3::tag::Tagged for Resource<u8> {
    const TAG: u8 = 1;
}

unsafe impl co3::tag::Tagged for Resource<u16> {
    const TAG: u8 = 2;
}

ffi! {
    #![unsafe(export("C"))]

    #[tag(u8)]
    type Resource<T>;

    impl<T> Drop for dyn Resource<T>
    where
        use<T> @ (<u8> | <u16>),
    {
        #[symbol_name = "mixed_drop"]
        fn drop(&mut self);
    }

    impl<T> Mixed<T> for dyn Resource<T>
    where
        use<T> @ (<u8> | <u16>),
    {
        #[symbol_name = "mixed_hidden"]
        fn hidden(&self);

        #[symbol_name = "mixed_exposed_{T}"]
        fn exposed(&self, value: &T);
    }
}

fn main() {}
