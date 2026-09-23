use co3::ffi;

fn exported() {}

struct Thing;

impl Thing {
    fn method() {}
}

ffi! {
    #![unsafe(export("C"))]

    pub fn exported();

    impl Thing {
        pub fn method();
    }
}

fn main() {}
