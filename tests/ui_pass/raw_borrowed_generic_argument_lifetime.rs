use co3::raw;

fn take<'a, T>(_value: &'a T) {}

struct Host;

impl Host {
    fn take<'a, T>(_value: &'a T) {}
}

raw! {
    fn take<'a, T>(value: &'a T);

    impl Host {
        fn take<'a, T>(value: &'a T);
    }
}

fn main() {
    let value = 7_u8;
    unsafe {
        take_raw::<u8>(&value);
        Host::take_raw::<u8>(&value);
    }
}
