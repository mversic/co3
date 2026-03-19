use co3::{export, export_, export_C};

co3::handles! {
    FfiStruct,
}

trait Kita {
    type T;

    extern "C" fn kita1(self);
}

#[export("C")]
#[derive(Clone)]
enum FfiStruct {
    A,
    B,
}

export_! {}

export_C! {
    #![abi = "C"]
}

export_! {
    #![abi = "Rust"]
    #![abi = "C"]
}

export_C! {
    trait Kita {
        fn kita(self);
    }
}

export_C! {
    enum Kita {}
}

export_C! {
    struct Kita {}
}

export_C! {
    union Kita {}
}

#[export]
type NoExportType = u32;

#[export]
trait NoExportTrait {}

#[export]
struct NoExportStruct {}

#[export]
enum NoExportEnum {}

#[export]
extern "C" fn kita3() {}

#[export]
impl FfiStruct {}

#[export("C")]
struct NoExportStruct<T>(T);

export_C! {
    #[unknown_attribute]
    fn kita3(_a: u32);
}

export_C! {
    #[some_attr]
    type OpaqueType;
}

export_C! {
    #[some_attr]
    impl Clone for FfiStruct {
        fn clone(&self) -> Self;
    }
}

export_C! {
    impl Kita for u32 {
        #[dispatch]
        fn kita(self);
    }
}

export_C! {
    #[dispatch]
    type OpaqueType;
}

export_C! {
    impl Kita for u32 {
        fn kita1(self) {}
    }
}

export_C! {
    fn kita1(a: u32) {}
}

export_C! {
    fn kita1((a, b): (u32, u32));
}

export_C! {
    type OpaqueType<T>;

    #[dispatch]
    impl Drop for OpaqueType {
        fn drop(&mut self);
    }

    #[dispatch(<u32, u8>)]
    impl<T> Clone for OpaqueType<T> {
        fn clone(&self);
    }
}

export_C! {
    #[dispatch(<'a>)]
    impl<'a> Kita<'a> {
        fn drop(&mut self);
    }
}

fn main() {}
