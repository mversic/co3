use co3::ffi;

fn local_function() {}

ffi! {
    #![unsafe(export("C"))]

    raw fn local_function();
}

fn main() {}
