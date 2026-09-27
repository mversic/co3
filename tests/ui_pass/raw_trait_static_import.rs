use co3::ffi;

struct Host;

trait Bridge<T> {
    unsafe extern "C" fn bridge(value: T) -> T;
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "raw_trait_"]

    impl<T> Bridge<T> for Host
    where
        use<T> @ (<u8> | <u16>),
    {
        raw fn bridge(value: move T) -> move T;
    }

    impl Host {
        raw fn plain(value: u8) -> u8;
        raw fn borrowed<'a>(value: &'a u8) -> &'a u8;
    }
}

fn main() {
    let _ = <Host as Bridge<u8>>::bridge;
    let _ = <Host as Bridge<u16>>::bridge;
    let _: unsafe extern "C" fn(u8) -> u8 = Host::plain;
    let _ = Host::borrowed;
}
