use co3::ffi;

struct Host;

ffi! {
    #![unsafe(export("C"))]
    raw unsafe fn unsafe_free();
}

ffi! {
    #![unsafe(export("C"))]
    raw fn too_many(
        a0: u8, a1: u8, a2: u8, a3: u8, a4: u8, a5: u8, a6: u8,
        a7: u8, a8: u8, a9: u8, a10: u8, a11: u8, a12: u8,
    );
}

ffi! {
    #![unsafe(export("C"))]
    raw extern "C" fn explicit_abi();
}

ffi! {
    #![unsafe(export("C"))]
    impl Host {
        raw unsafe fn unsafe_method(&self);
    }
}

ffi! {
    #![unsafe(export("C"))]
    impl Host {
        raw extern "C" fn explicit_method(&self);
    }
}

fn main() {}
