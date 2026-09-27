use co3::ffi;

ffi! {
    #![unsafe(export("C"))]
    type Alias = raw fn();
}

fn main() {}
