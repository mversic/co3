use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(Clone, RustSpec)]
#[rust_spec(opaque)]
struct Opaque(u32);

#[derive(Clone, RustSpec, ReprC)]
struct Kita5(u32, Opaque);

fn takes_kita5(_value: &Kita5) {}
fn takes_kita5_mut(_value: &mut Kita5) {}
fn takes_kita5_slice(_value: &[Kita5]) {}
fn takes_kita5_slice_mut(_value: &mut [Kita5]) {}

ffi! {
    #![unsafe(export("C"))]

    type Opaque;

    fn takes_kita5(#[soft] value: &Kita5);
    fn takes_kita5_mut(#[soft] value: &mut Kita5);
    fn takes_kita5_slice(#[soft] value: &[Kita5]);
    fn takes_kita5_slice_mut(#[soft] value: &mut [Kita5]);
}

fn main() {}
