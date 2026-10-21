//! A borrowed selected input must not repeat concrete encoding bounds on the
//! dispatch method implementation. The view store is empty for each format.

use std::marker::PhantomData;
use co3::{ReprC, ffi, rust_spec::RustSpec};

#[derive(RustSpec, ReprC)]
struct Image<Format: ?Sized> {
    bytes: Vec<u8>,
    format: PhantomData<Format>,
}

ffi! {
    #![unsafe(extern("C"))]

    type Transcoder;

    #[tag(u8, unsafe(1))]
    type Png;
    #[tag(u8, unsafe(2))]
    type Jpeg;

    impl Transcoder {
        #[symbol_name = "convert_to_{To}"]
        fn convert<dyn(u8) From, To>(&self, image: Image<From>) -> move Image<To>
        where
            use<From, To> @ (<Png, Jpeg> | <Jpeg, Png>);
    }
}

fn convert(transcoder: &Transcoder, image: Image<Png>) -> Image<Jpeg> {
    transcoder.convert::<Png, Jpeg>(image)
}

fn main() {}
