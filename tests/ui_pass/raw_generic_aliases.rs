use co3::{ReprC, ffi};

ffi! {
    #![unsafe(export("C"))]

    type Moved<T> = raw extern "C" fn(move T) -> move T;
    type Borrowed<T> = raw extern "C" fn(T) -> T;
    type Array<const N: usize> = raw extern "C" fn(move [u8; N]) -> move [u8; N];
    type Combined<T, const N: usize> = raw extern "C" fn(move [T; N]);
}

fn main() {
    type StringC = <String as ReprC>::CType;
    static_assertions::assert_type_eq_all!(Moved<String>, unsafe extern "C" fn(StringC) -> StringC);
    static_assertions::assert_type_eq_all!(Borrowed<u8>, unsafe extern "C" fn(u8) -> u8);
    static_assertions::assert_type_eq_all!(Array<2>, unsafe extern "C" fn([u8; 2]) -> [u8; 2]);
    static_assertions::assert_type_eq_all!(Combined<u8, 2>, unsafe extern "C" fn([u8; 2]));
}
