use co3::ffi;

trait Kita {
    fn kita(self) -> u32;
}

trait RefKita {
    fn kita(&self) -> u32;
}

struct Exported0(u8);
impl Kita for Exported0 {
    fn kita(self) -> u32 {
        unimplemented!()
    }
}
impl RefKita for Exported0 {
    fn kita(&self) -> u32 {
        unimplemented!()
    }
}

struct Exported1(u8);
impl Kita for Exported1 {
    fn kita(self) -> u32 {
        unimplemented!()
    }
}
impl RefKita for Exported1 {
    fn kita(&self) -> u32 {
        unimplemented!()
    }
}

ffi! {
    #![unsafe(export("C"))]

    impl<dyn(u8) T> Kita for T
    where
        use<T> @ <u32>,
    {
        fn kita(self, self_id: <dyn T>::TAG) -> u32;
    }
}

ffi! {
    #![unsafe(export("C"))]

    fn never<dyn(u8) T>()
    where
        use<T> @ ();
}

ffi! {
    #![unsafe(extern("C"))]

    fn never<dyn(u8) T>()
    where
        use<T> @ ();
}

ffi! {
    #![unsafe(export("C"))]

    impl<dyn(u8) T> Dispatch for T
    where
        use<T> @ <u32> | <u64>,
    {
        fn dispatch(&self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(u8) T> Dispatch for T
    where
        use<T> @ <u32> | <u64>,
    {
        fn dispatch(&self);
    }
}

ffi! {
    #![unsafe(export("C"))]

    fn dispatch<dyn(u8) T>()
    where
        use<T> @ <u32>,
        T: Copy;
}

ffi! {
    #![unsafe(extern("C"))]

    fn dispatch<dyn(u8) T>()
    where
        use<T> @ <u32>,
        T: Copy;
}

ffi! {
    #![unsafe(export("C"))]

    impl<'a, dyn(u8) T> Kita<'a> for T
    where
        use<T> @ <'a, u32>,
    {
        fn drop(&mut self);
    }
}

ffi! {
    #![symbol_prefix = "kita"]

    #![unsafe(extern("C"))]

    impl<'a, dyn(u8) T> Kita<'a> for T
    where
        use<T> @ <'a, u32>,
    {
        fn drop(&mut self, self_id: <dyn Self>::TAG);
    }
}

ffi! {
    #![unsafe(export("C"))]

    impl<dyn T> Kita for T
    where
        use<T> @ <u32>,
    {
        fn kita(self) -> u32;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn T> Kita for T
    where
        use<T> @ <u32>,
    {
        #[symbol_name = "kita"]
        fn kita(self) -> u32;
    }
}

ffi! {
    #![unsafe(export("C"))]

    impl<T> Kita for dyn T
    where
        use<T> @ <u32>,
    {
        fn kita(self) -> u32;
    }
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(i64) T> Kita for T
    where
        use<T> @ <u32>,
    {
        #[symbol_name = "kita"]
        fn kita(self, self_id: <dyn Self>::TAG) -> <dyn T>::TAG;
    }

    fn kita(self, self_id: <dyn Self>::TAG) -> <dyn T>::TAG;
}

ffi! {
    #![unsafe(extern("C"))]

    impl<dyn(u64) T> Kita for T
    where
        use<T> @ <u32>,
    {
        #[symbol_name = "kita"]
        fn kita(self, self_id: (<dyn Self>::TAG,)) -> u32;
    }
}

ffi! {
    #![unsafe(extern("C"))]
    #![symbol_prefix = "kita"]

    #[tag(u32, unsafe(1))]
    type Externed1;

    impl<dyn(u32) T> RefKita for T
    where
        use<T> @ (<Externed1> | <Externed1>),
    {
        #[symbol_name = "kita"]
        fn kita(self_id: <dyn T>::TAG, &self) -> u32;
    }
}

ffi! {
    #![unsafe(export("C"))]

    #[tag(u8, unsafe(1))]
    type Exported;

    impl Trait for dyn Exported + Send {
        fn method(&self);
    }
}

ffi! {
    #![unsafe(extern("C"))]

    #[tag(u8, unsafe(1))]
    type Imported;

    impl Trait for dyn Imported + Send {
        fn method(&self);
    }
}

fn main() {}
