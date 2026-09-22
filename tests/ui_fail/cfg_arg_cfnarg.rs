use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]

    fn bad(#[cfg(all())] value: ());
}

fn main() {}
