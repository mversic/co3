use co3::ffi;

struct Host;

trait Service {
    fn selected<T>()
    where
        (): host_selected::DispatchSet<T>;
}

ffi! {
    #![unsafe(extern("C"))]

    impl Service for Host {
        fn selected<T>()
        where
            use<T> @ <u8>;
    }
}

fn main() {
    let _ = <Host as Service>::selected::<u8>;
}
