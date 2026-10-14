use core::cell::UnsafeCell;
use co3::ffi;

#[derive(co3::rust_spec::RustSpec)]
#[rust_spec(opaque)]
struct DropOpaque(UnsafeCell<u32>);

impl Drop for DropOpaque {
    fn drop(&mut self) {}
}

#[derive(co3::rust_spec::RustSpec)]
#[rust_spec(opaque)]
struct MutRefOpaque<'a>(&'a mut u32);

ffi! {
    #![unsafe(export("C"))]

    type DropOpaque;
    type MutRefOpaque<'a>;
}

fn main() {
    let dropped = DropOpaque(UnsafeCell::new(1));
    let _ = co3::encode::<&DropOpaque>(&dropped);

    let mut value = 1;
    let mut_ref = MutRefOpaque(&mut value);
    let _ = co3::encode::<&MutRefOpaque<'_>>(&mut_ref);
}
