use core::marker::PhantomData;

use co3::{ExternC, ReprC, Tag, encode, ffi, rust_spec::RustSpec, slice::Unpack2};

trait Prop {
    type DefinedBy;
}

enum OdbcDefined {}

#[derive(Tag)]
#[tag(u8, unsafe(1))]
enum Attribute {}

impl Prop for Attribute {
    type DefinedBy = OdbcDefined;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct AttrLength<D>(u16, PhantomData<fn() -> D>);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct AttrPointer<D>(u32, PhantomData<fn() -> D>);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Parts<D>(u32, u16, PhantomData<fn() -> D>);

impl<D> Unpack2<u32, <AttrLength<D> as ExternC>::CType> for Parts<D> {
    type Error = core::convert::Infallible;
    fn unpack(
        value: Self::CType,
    ) -> Result<(u32, <AttrLength<D> as ExternC>::CType), Self::Error> {
        Ok((value.0, encode(AttrLength(value.1, PhantomData))))
    }
}

impl<D>
    Unpack2<
        <AttrPointer<D> as ExternC>::CType,
        <AttrLength<D> as ExternC>::CType,
    > for Parts<D>
{
    type Error = core::convert::Infallible;
    fn unpack(
        value: Self::CType,
    ) -> Result<
        (
            <AttrPointer<D> as ExternC>::CType,
            <AttrLength<D> as ExternC>::CType,
        ),
        Self::Error,
    > {
        Ok((
            encode(AttrPointer(value.0, PhantomData)),
            encode(AttrLength(value.1, PhantomData)),
        ))
    }
}

mod symbols {
    #[unsafe(no_mangle)]
    extern "C" fn projected(_: u8, _: u32, _: u16) {}

    #[unsafe(no_mangle)]
    extern "C" fn projected_try(_: u8, _: u32, _: u16) {}

    #[unsafe(no_mangle)]
    extern "C" fn both_parts(_: u32, _: u16) {}
}

ffi! {
    #![unsafe(extern("C"))]

    #[symbol_name = "projected"]
    fn projected<dyn(u8) A: Prop>(
        attribute: move <dyn A>::TAG,
        #[unpack(u32, AttrLength<<A as Prop>::DefinedBy> => u16)]
        value: move Parts<<A as Prop>::DefinedBy>,
    )
    where
        use<A> @ <Attribute>;

    #[symbol_name = "projected_try"]
    fn projected_try<dyn(u8) A: Prop>(
        attribute: move <dyn A>::TAG,
        #[unpack(u32, AttrLength<<A as Prop>::DefinedBy> => u16)]
        value: move Parts<<A as Prop>::DefinedBy>,
    )
    where
        use<A> @ <Attribute>;

    #[symbol_name = "both_parts"]
    fn both_parts(
        #[unpack(AttrPointer<OdbcDefined> => u32, AttrLength<OdbcDefined> => u16)]
        value: move Parts<OdbcDefined>,
    );
}

fn main() {
    projected(Parts(7, 11, PhantomData));
    projected_try(Parts(7, 11, PhantomData));
    both_parts(Parts(7, 11, PhantomData));
}
