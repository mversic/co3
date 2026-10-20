use co3::{ReprC, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
struct UninitField(core::mem::MaybeUninit<u8>);

#[derive(RustSpec, ReprC)]
enum UninitVariant {
    Field(core::mem::MaybeUninit<u8>),
    Empty,
}

static_assertions::assert_not_impl_any!(<UninitField as ReprC>::CType: PartialEq);
static_assertions::assert_not_impl_any!(<UninitVariant as ReprC>::CType: PartialEq);

fn main() {
    validation::check();
}

mod validation {
    use co3::{ReprC, rust_spec::RustSpec};

    #[derive(RustSpec, ReprC)]
    #[repr_c(is_valid = |value| value.is_power_of_two())]
    struct PowerOfTwo(u32);

    #[derive(RustSpec, ReprC)]
    #[repr(u8)]
    enum Number {
        #[repr_c(allow_zero_tag)]
        #[repr_c(is_valid = |value| value.is_power_of_two())]
        PowerOfTwo(u32),
        Other,
    }

    pub(super) fn check() {
        let _ = PowerOfTwo(2);
        let _ = Number::PowerOfTwo(2);
    }
}
