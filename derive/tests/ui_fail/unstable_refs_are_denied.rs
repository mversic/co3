use co3::export;

#[export("C")]
pub extern "C" fn take_unstable_ref(arg: &(u8,)) -> u8 {
    arg.0
}

fn main() {}
