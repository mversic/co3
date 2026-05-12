#[derive(Clone, Copy)]
struct Unit;

struct UnsizedRobust([u8]);
struct UnsizedCloned([(u8,)]);

struct NoDropUnsizedTransmuted([u8]);
struct NeedsDropSizedTransmuted(u8);
impl Drop for NeedsDropSizedTransmuted {
    fn drop(&mut self) {}
}

co3::reprC! {
    unsafe impl Robust for Unit {}
}

co3::reprC! {
    unsafe impl SizedRobust for UnsizedRobust {}
}

co3::reprC! {
    impl SizedCloned for UnsizedCloned {}
}

co3::reprC! {
    unsafe impl NoDropSizedTransmuted for NoDropUnsizedTransmuted {
        type Target = u8;

        fn is_valid(_target: &Self::Target) -> bool {
            true
        }
    }
}

co3::reprC! {
    unsafe impl NoDropSizedTransmuted for NeedsDropSizedTransmuted {
        type Target = [u8];

        fn is_valid(_target: &Self::Target) -> bool {
            true
        }
    }
}

fn main() {}
