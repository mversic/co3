use co3::raw;

fn accept(_callback: for<'a> unsafe extern "C" fn()) {}

raw! {
    pub fn accept(callback: for<'a> unsafe extern "C" fn());
}

fn main() {}
