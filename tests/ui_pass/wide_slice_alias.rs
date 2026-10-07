use co3::{ReprC, rust_spec::RustSpec, wide::Wide};

type Bytes = [u8];

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct WithAliasedTail {
    head: u32,
    tail: Bytes,
}

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct WithSliceTail {
    head: u32,
    tail: [u8],
}

fn assert_wide<T: Wide + ?Sized>() {}

fn main() {
    assert_wide::<WithAliasedTail>();
    assert_wide::<WithSliceTail>();

    // Aliases and their underlying slice type must produce the same fixed header.
    let _: [(); core::mem::size_of::<WithAliasedTailData>()] =
        [(); core::mem::size_of::<WithSliceTailData>()];
}
