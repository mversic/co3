use co3::raw;

fn callback_target(_: &u32) {}

raw! {
    fn callback_target(#[soft] value: &mut u32);
}

fn main() {}
