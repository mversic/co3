use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct NoDrop(u32);

fn require_repr_c<T: ReprC>() {}

fn main() {
    require_repr_c::<NoDropView<'static>>();
}
