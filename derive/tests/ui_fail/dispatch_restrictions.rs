use co3::{export_C, extern_C, handles};

pub trait Unimplemented {}

trait Kita {
    fn kita(self) -> u32;
}

struct Exported<T>(T);

impl Kita for Exported<u32> {
    fn kita(self) -> u32 {
        unimplemented!()
    }
}

handles! {
    Exported<u32>,
    Externed<u32>
}

export_C! {
    #[dispatch(<u32>)]
    impl<dyn(u8) T> Kita for T {
        fn kita(self, self_id: <dyn T>::ID) -> u32;
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<u32>)]
    impl<dyn(u32) T> Kita for T {
        #[link_name = "kita"]
        fn kita(&self) -> u32;
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<dyn(String) T> Kita for T {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<u32>)]
    impl<dyn(String) T> Kita for T {
        #[link_name = "kita"]
        fn kita(&self, self_id: <dyn Self>::ID) -> u32;
    }
}

export_C! {
    #[dispatch(<'a, u32>)]
    impl<'a, dyn(u8) T> Kita<'a> for T {
        fn drop(&mut self);
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<'a, u32>)]
    impl<'a, dyn(u8) T> Kita<'a> for T {
        fn drop(&mut self, self_id: <dyn Self>::ID);
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for T {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for T {
        #[link_name = "kita"]
        fn kita(self) -> u32;
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<dyn T> Kita for T {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #[dispatch(<u32>)]
    impl<dyn T> Kita for T {
        #[link_name = "kita"]
        fn kita(self) -> u32;
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for dyn T {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for dyn T {
        #[link_name = "kita"]
        fn kita(self) -> u32;
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for dyn u32 {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<u32>)]
    impl<T> Kita for dyn u32 {
        #[link_name = "kita"]
        fn kita(&self, self_id: T::ID) -> u32;
    }
}

export_C! {
    #[dispatch(<u32>)]
    impl<T> Kita for dyn Option<T> {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #![link(crate = "kita")]

    #[dispatch(<u32>)]
    impl<T> Kita for dyn Option<T> {
        #[link_name = "kita"]
        fn kita(&self, self_id: T::ID) -> u32;
    }
}

extern_C! {
    #[dispatch(<u32>)]
    impl<dyn(i64) T> Kita for T {
        #[link_name = "kita"]
        fn kita(self, self_id: <dyn Self>::ID) -> <dyn T>::ID;
    }
}

// TODO: This produces extra unrelated error message
extern_C! {
    #[dispatch(<u32>)]
    impl<dyn(u64) T> Kita for T {
        #[link_name = "kita"]
        fn kita(self, self_id: (<dyn Self>::ID,)) -> u32;
    }
}

export_C! {
    #[id(u8)]
    type Exported<T>;

    #[dispatch(<u32>)]
    impl<T> Drop for dyn Exported<T> {
        fn drop(&mut self);
    }

    #[dispatch(<Exported<u32>>)]
    impl<dyn(u8) T: Unimplemented> Kita for T where i32: Unimplemented {
        fn kita(self) -> u32;
    }
}

extern_C! {
    #[id(u64)]
    type Externed<T>;

    #[dispatch]
    impl<T> Drop for dyn Externed<T> {
        #[link_name = "drop"]
        fn drop(&mut self, self_id: <dyn Externed<T>>::ID);
    }

    #[dispatch(<Externed<u32>>)]
    impl<dyn(u64) T: Unimplemented> Kita for T where i32: Unimplemented {
        #[link_name = "kita"]
        fn kita(self, self_id: <dyn Self>::ID) -> u32;
    }
}

fn main() {}
