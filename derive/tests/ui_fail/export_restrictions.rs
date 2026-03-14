use co3::{ReprC, export, export_, export_C};

co3::handles! {
    FfiStruct,
}

#[export]
type NoExportType = u32;

#[export]
trait NoExportTrait {}

#[export]
enum NoExportEnum {}

#[export]
struct NoExportStruct {}

trait Kita {
    type T;

    extern "system" fn kita0(self, a: &u8);
    extern "C" fn kita1(self);
    fn kita2(self);
}

#[derive(Clone, ReprC)]
#[reprC(opaque)]
enum FfiStruct {
    A,
    B,
}

export_! {}

export_! {
    #![abi = "Rust"]
    #[dispatch]
    fn kita();
}

#[export("C")]
#[export(skip)]
extern "C" fn kita1() {}

#[export]
extern "C" fn kita3() {}

#[export]
impl FfiStruct {}

export_C! {
    trait Kita {
        fn kita2(self);
    }
}

#[export("C")]
impl Kita for FfiStruct {
    type T = u32;

    #[export]
    fn kita0(self, _a: &u8) {}
    #[unsafe(no_mangle)]
    #[export(skip)]
    fn kita1(self) {}
    #[export(skip)]
    #[unsafe(export_name = "kita")]
    extern "C" fn kita2(self) {}
}

#[export("C")]
impl FfiStruct {
    fn kita(_a: *const u32) {}

    #[unsafe(no_mangle)]
    #[export(skip)]
    extern "C" fn kita1(self) {}
    #[export(skip)]
    #[unsafe(export_name = "kita")]
    pub extern "C" fn kita2(self) {}
}

export_C! {
    #[unknown_attribute]
    impl Clone for FfiStruct {
        fn clone(&self) -> Self;
    }
}

export_C! {
    #[unknown_attribute]
    fn kita3(_a: u32);
}

export_C! {
    #[unsafe(no_mangle)]
    impl Clone for FfiStruct {
        fn clone(&self) -> Self;
    }
}

export_C! {
    #[unsafe(export_name = "clone")]
    impl Clone for FfiStruct {
        fn clone(&self) -> Self;
    }
}

export_C! {
    trait Kita {
        type T;

        fn kita2(self);
    }
}

export_C! {
    #[dispatch(
        Self = [FfiStruct],
        Self = [u32],
    )]
    trait Kita {
        fn kita1(self);
    }
}

// TODO: I think multiple entries can be allowed, but args can't be duplicated?
export_C! {
    #[dispatch(Self = [FfiStruct])]
    #[dispatch(Self = [u32])]
    trait Kita {
        fn kita1(self);
    }
}

export_C! {
    #[dispatch]
    impl Kita for u32 {
        fn kita2(self);
    }
}

fn main() {}
