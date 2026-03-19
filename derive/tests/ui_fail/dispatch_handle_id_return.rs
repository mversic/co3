use co3::{export_C, extern_C};

trait Kita {
    fn kita(self);
}

export_C! {
    #[dispatch(<u32>)]
    impl<dyn(u8) T: co3::handle::Handle> Kita for T {
        fn kita(self) -> T::ID;
    }
}

extern_C! {
    #[dispatch(<u32>)]
    impl<#[erased(i64)] T: co3::handle::Handle> Kita for T {
        #[link_name = "kita"]
        fn kita(self) -> T::ID;
    }
}

fn main() {}
