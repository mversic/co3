use co3::{CFnArg, CFnReturn, ReprC, CType, rust_spec::RustSpec};

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct ParamReprCZst<T: ?Sized> {
    a: (),
    b: T,
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
struct ParamTransparentZst<T: ?Sized> {
    b: T,
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
enum ParamTransparentEnum<T> {
    A(T),
}

#[derive(Clone, Copy, RustSpec, ReprC)]
enum ParamFieldlessEnum {
    A,
}

#[derive(Clone, Copy, RustSpec, ReprC)]
struct ZeroLenArrayZst {
    field: [u8; 0],
}

#[derive(Clone, Copy, RustSpec, ReprC)]
struct ParamNoReprZst<T: ?Sized> {
    a: (),
    b: T,
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(C)]
struct ReprCZst {
    a: (),
    b: (),
}

#[derive(Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
struct TransparentZst {
    b: (),
}

#[derive(Clone, Copy, RustSpec, ReprC)]
struct NoReprZst {
    a: (),
    b: (),
}

#[derive(Debug, Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
pub enum FieldlessTransparentEnum {
    A,
}

#[derive(Debug, Clone, Copy, RustSpec, ReprC)]
#[repr(transparent)]
pub struct UnitTransparentStruct;

fn require_arg<T: CFnArg>() {}
fn require_return<T: CFnReturn>() {}

fn main() {
    require_arg::<<ZeroLenArrayZst as ReprC>::CType>();
    require_return::<<ZeroLenArrayZst as ReprC>::CType>();

    require_arg::<<ReprCZst as ReprC>::CType>();
    require_return::<<ReprCZst as ReprC>::CType>();

    require_arg::<<TransparentZst as ReprC>::CType>();
    require_return::<<TransparentZst as ReprC>::CType>();

    require_arg::<<NoReprZst as ReprC>::CType>();
    require_return::<<NoReprZst as ReprC>::CType>();

    require_arg::<<ParamReprCZst<()> as ReprC>::CType>();
    require_return::<<ParamReprCZst<()> as ReprC>::CType>();

    require_arg::<<ParamTransparentZst<()> as ReprC>::CType>();
    require_return::<<ParamTransparentZst<()> as ReprC>::CType>();

    require_arg::<<ParamNoReprZst<()> as ReprC>::CType>();
    require_return::<<ParamNoReprZst<()> as ReprC>::CType>();

    require_arg::<<ParamTransparentEnum<()> as ReprC>::CType>();
    require_return::<<ParamTransparentEnum<()> as ReprC>::CType>();

    require_arg::<<ParamTransparentEnum<u32> as ReprC>::CType>();
    require_return::<<ParamTransparentEnum<u32> as ReprC>::CType>();

    require_arg::<<ParamFieldlessEnum as ReprC>::CType>();
    require_return::<<ParamFieldlessEnum as ReprC>::CType>();
}
