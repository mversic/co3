use co3::{ffi, rust_spec::RustSpec};

trait Value {
    fn value(&self) -> u8;
}

mod export {
    use super::*;

    #[derive(RustSpec)]
    #[rust_spec(opaque)]
    struct Concrete(u8);

    impl Value for Concrete {
        fn value(&self) -> u8 {
            self.0
        }
    }

    impl Concrete {
        fn own_value(&mut self) -> u8 {
            self.0
        }
    }

    ffi! {
        #![unsafe(export("C"))]

        #[tag(u8, unsafe(1))]
        type Concrete;

        impl Value for dyn Concrete {
            #[symbol_name = "concrete_dyn_self_value"]
            fn value(&self) -> u8;
        }

        impl dyn Concrete {
            #[symbol_name = "concrete_dyn_self_own_value"]
            fn own_value(&mut self) -> u8;
        }
    }
}

mod import {
    use super::*;

    ffi! {
        #![unsafe(extern("C"))]

        #[tag(u8, unsafe(1))]
        type Concrete;

        impl Value for dyn Concrete {
            #[symbol_name = "concrete_dyn_self_value"]
            fn value(tag_id: <dyn Self>::TAG, &self) -> u8;
        }

        impl dyn Concrete {
            #[symbol_name = "concrete_dyn_self_own_value"]
            fn own_value(tag_id: <dyn Concrete>::TAG, &mut self) -> u8;
        }
    }
}

fn main() {}
