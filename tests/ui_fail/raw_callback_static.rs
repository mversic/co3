use co3::{ffi, raw};

fn progress(_: &'static str, _: f32) {}

raw! {
    fn progress(stage: &'static str, fraction: f32);
}

ffi! {
    #![unsafe(extern("C"))]

    fn register(callback: raw fn(&str, f32));
}

fn main() {
    register(progress_raw);
}
