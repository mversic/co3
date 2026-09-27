use co3::raw;

fn borrowed_generic<'a, T>(value: &'a T) -> &'a T {
    value
}

raw! {
    fn borrowed_generic<'a, T>(value: &'a T) -> &'a T;
}

fn main() {}
