use core::marker::PhantomData;

use co3::{ReprC, borrow::Borrow, rust_spec::RustSpec};

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

fn assert_transparent_identity<T: 'static>()
where
    TransparentMarker<T>: Borrow<Borrowed<'static> = TransparentMarker<T>, Owner = ()>,
    TransparentStaticMarker: Borrow<Borrowed<'static> = TransparentStaticMarker, Owner = ()>,
{
}

fn main() {
    assert_transparent_identity::<u8>();

    let _ = TransparentWithData::<u8>(0, PhantomData);
    let _ = CStaticMarker(PhantomData);
}
