use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]
    static mut VALUE: Box<u8>;
}

fn main() {
    let _take = || unsafe { VALUE.take() };
}
