use co3::{ReprC, ffi};

ffi! {
    #![unsafe(extern("C"))]

    type ExplicitAbi = raw extern "system" fn(u8);
    type InheritedAbi = raw fn(u8);
    type Ordinary = core::num::NonZeroU8;
    type Regular = extern "C" fn(u8);
    type SharedUnpacked = extern "C" fn(#[unpack(_, _)] &[u8]) -> usize;
    type InteriorUnpacked = raw extern "C" fn(#[unpack(*mut core::cell::Cell<u8>, usize)] &[core::cell::Cell<u8>]) -> usize;
    type RegularUnpacked = extern "C" fn(#[unpack(_, _)] &mut [u8]) -> usize;
    type Composite = (String, raw fn(String));
    type MaybeCallback = Option<raw fn(u8)>;
    type NicheOk = Result<core::num::NonZeroU8, ()>;
    type NicheErr = Result<(), core::num::NonZeroU8>;
    type CompositeReturn = (u8, raw fn() -> u8);
    type Nested = raw extern "C" fn(raw extern "system" fn(u8));
    type Returned = raw fn() -> raw fn(u8);
    type RawNested = raw extern "C" fn(extern "system" fn(u8));
}

ffi! {
    #![unsafe(extern("C"))]

    fn interior_slice(#[unpack(*mut core::cell::Cell<u8>, usize)] values: &[core::cell::Cell<u8>]) -> usize;
}

ffi! {
    #![unsafe(extern("C"))]

    type BadArg = raw extern "C" fn(move [u8; 2]);
    type BadReturn = raw extern "C" fn() -> move [u8; 2];
}

ffi! {
    #![unsafe(export("C"))]

    type Moved<T> = raw extern "C" fn(move T) -> move T;
    type Borrowed<T> = raw extern "C" fn(T) -> T;
    type Array<const N: usize> = raw extern "C" fn(move [u8; N]) -> move [u8; N];
    type Combined<T, const N: usize> = raw extern "C" fn(move [T; N]);
}

fn main() {
    let _: Option<ExplicitAbi> = None;
    let _: Option<InheritedAbi> = None;
    let _: Option<Regular> = None;
    let _: Option<SharedUnpacked> = None;
    let _: Option<InteriorUnpacked> = None;
    let _: Option<RegularUnpacked> = None;
    let _: Option<Composite> = None;
    let _: Option<MaybeCallback> = None;
    let _: Option<NicheOk> = None;
    let _: Option<NicheErr> = None;
    let _: Option<CompositeReturn> = None;
    let _: Option<Nested> = None;
    let _: Option<Returned> = None;
    let _: Option<RawNested> = None;
    let _: Option<Moved<String>> = None;
    let _: Option<Borrowed<u8>> = None;
    let _: Option<Array<2>> = None;
    let _: Option<Combined<u8, 2>> = None;
    static_assertions::assert_not_impl_any!(BadArg: ReprC);
    static_assertions::assert_not_impl_any!(BadReturn: ReprC);
}
