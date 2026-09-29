use co3::ffi;

fn local_function() {}

struct Local(u8);

impl Local {
    fn method(&self) {}
}

ffi! {
    #![unsafe(export("C"))]

    raw fn local_function();

    type Local;

    impl Local {
        raw fn method(&self);
    }
}

fn main() {}
