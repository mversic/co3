use co3::export_C;

trait Kita {
    fn kita(self);
}

export_C! {
    #[dispatch(<u32>)]
    impl<T: co3::handle::Handle> Kita for T {
        fn kita(self);
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<dyn T: co3::handle::Handle> Kita for T {
        fn kita(self);
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<dyn(bool) T: co3::handle::Handle> Kita for T {
        fn kita(self);
    }
}

fn main() {}
