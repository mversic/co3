use co3::ReprC;

#[derive(ReprC)]
#[repr_c(transparent)]
enum RustLayout {
    Value(u32),
}

#[derive(ReprC)]
#[repr(C)]
#[repr_c(transparent)]
enum CLayout {
    Value(u32),
}

#[derive(ReprC)]
#[repr(u8)]
#[repr_c(transparent)]
enum PrimitiveLayout {
    Value(u32),
}

fn main() {}
