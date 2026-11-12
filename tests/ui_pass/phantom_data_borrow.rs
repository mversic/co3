use core::marker::PhantomData;

use co3::{ReprC, raw, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct TransparentMarker<T>(PhantomData<T>);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct TransparentStaticMarker(PhantomData<()>);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct TransparentWithData<T>(u32, PhantomData<T>);

#[derive(RustSpec, ReprC)]
struct CStaticMarker(PhantomData<()>);

fn accept_marker<T>(_value: TransparentMarker<T>) {}
fn accept_static_marker(_value: TransparentStaticMarker) {}

raw! {
    fn accept_marker<T>(value: TransparentMarker<T>);
    fn accept_static_marker(value: TransparentStaticMarker);
}

fn main() {
    let _ = accept_marker_raw::<u8>;
    let _ = accept_static_marker_raw;

    let _ = TransparentWithData::<u8>(0, PhantomData);
    let _ = CStaticMarker(PhantomData);
}
