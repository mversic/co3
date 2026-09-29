use co3::ffi;

fn exported() {}

struct Thing;

impl Thing {
    fn method() {}
}

trait Behavior {
    fn act();
}

impl Behavior for Thing {
    fn act() {}
}

ffi! {
    #![unsafe(export("C"))]

    pub fn exported();

    impl Thing {
        pub fn method();
    }

    impl Behavior for Thing {
        pub fn act();
    }
}

fn main() {}
