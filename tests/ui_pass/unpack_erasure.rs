use core::marker::PhantomData;

use co3::{ReprC, Tag, encode, ffi, lang, pack::Unpack2, rust_spec::RustSpec};

trait Prop {
    type DefinedBy;
}

#[derive(RustSpec, ReprC)]
struct OdbcDefined;

#[derive(Tag)]
#[tag(u8, unsafe(1))]
enum Attribute {}

impl Prop for Attribute {
    type DefinedBy = OdbcDefined;
}

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct AttrLength<D>(u16, PhantomData<D>);

#[derive(RustSpec, ReprC)]
#[repr(transparent)]
struct AttrPointer<D>(u32, PhantomData<D>);

#[derive(RustSpec, ReprC)]
#[repr(C)]
struct Parts<D>(u32, u16, PhantomData<D>);

impl<D: ReprC> Unpack2<u32, <AttrLength<D> as ReprC>::CType> for Parts<D> {
    type Error = core::convert::Infallible;
    fn unpack(value: Self::CType) -> Result<(u32, <AttrLength<D> as ReprC>::CType), Self::Error> {
        Ok((value.0, unsafe {
            encode::<_, lang::C>(AttrLength::<D>(value.1, PhantomData))
        }))
    }
}

impl<D: ReprC> Unpack2<<AttrPointer<D> as ReprC>::CType, <AttrLength<D> as ReprC>::CType>
    for Parts<D>
{
    type Error = core::convert::Infallible;
    fn unpack(
        value: Self::CType,
    ) -> Result<
        (
            <AttrPointer<D> as ReprC>::CType,
            <AttrLength<D> as ReprC>::CType,
        ),
        Self::Error,
    > {
        Ok((
            unsafe { encode::<_, lang::C>(AttrPointer::<D>(value.0, PhantomData)) },
            unsafe { encode::<_, lang::C>(AttrLength::<D>(value.1, PhantomData)) },
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
