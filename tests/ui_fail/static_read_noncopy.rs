use co3::ffi;

ffi! {
    #![unsafe(extern("C"))]
    static mut VALUE: Box<u8>;
}

fn main() {
    let _ = unsafe { VALUE.read() };
    let _ = unsafe { VALUE.soft_read(&mut ()) };
}
