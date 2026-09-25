use co3::{ReprC, rust_spec::RustSpec, wide::Wide};

type Bytes = [u8];

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct NonTransparentDst(Bytes);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct GenericNonTransparentDst<T: ?Sized>(T);

fn require_wide<T: Wide + ?Sized>() {}

fn main() {
    require_wide::<NonTransparentDst>();
    let _: NonTransparentDstData;
    require_wide::<GenericNonTransparentDst<[u8]>>();
    let _: GenericNonTransparentDstData<[u8]>;
}
