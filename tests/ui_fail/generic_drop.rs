use co3::ReprC;
use co3::rust_spec::RustSpec;

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct ZDropRegression<T>(core::marker::PhantomData<T>);

impl<T> Drop for ZDropRegression<T> {
    fn drop(&mut self) {}
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct ZCopyDropRegression<T: Copy>(T);

impl<T: Copy> Drop for ZCopyDropRegression<T> {
    fn drop(&mut self) {}
}

fn main() {}
