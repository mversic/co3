use co3::FfiType;

#[derive(FfiType)]
pub struct Hello {
    a: i32,
    b: i32,
}

#[co3::carbonate]
impl Hello {
    pub fn hello(Hello { a: a1, b: b1 }: Hello, Hello { a: a2, b: b2 }: Hello) -> i32 {
        a1 + b1 + a2 + b2
    }
}

fn main() {
    Hello::hello(Hello { a: 1, b: 2 }, Hello { a: 1, b: 2 });
}
