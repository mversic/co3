#[unsafe(export_name = "co3_static__VERSION")]
pub static VERSION: <u32 as co3::ReprC>::CType = 7;
#[unsafe(export_name = "co3_static_flags")]
pub static mut FLAGS: <u32 as co3::ReprC>::CType = 0;

mod imported {
    use co3::ffi;

    ffi! {
        #![unsafe(extern("C"))]
        #![symbol_prefix = "co3_static"]

        pub static VERSION: u32;
        #[symbol_name = "co3_static_flags"]
        pub static mut FLAGS: u32;
    }
}

#[test]
fn immutable_static_has_the_imported_abi() {
    assert_eq!(VERSION, 7);
    assert_eq!(*imported::VERSION.get().unwrap(), 7);
    assert_eq!(*unsafe { imported::VERSION.get_unchecked() }, 7);
    assert_eq!(imported::VERSION.read(), Some(7));
}

#[test]
fn mutable_static_has_the_imported_abi() {
    unsafe {
        imported::FLAGS.set(3);
        let flags = imported::FLAGS.read().unwrap();
        assert_eq!(flags, 3);
        let flags = FLAGS;
        assert_eq!(flags, 3);
        assert_eq!(imported::FLAGS.take(), Some(3));
        let flags = FLAGS;
        assert_eq!(flags, 0);
    }
}
