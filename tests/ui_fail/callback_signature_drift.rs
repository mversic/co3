use co3::ffi;

fn callback_target(_: &u32) {}

ffi! {
    #![unsafe(extern("C"))]

    raw fn callback_target(#[soft] value: &mut u32);
}

fn main() {}
