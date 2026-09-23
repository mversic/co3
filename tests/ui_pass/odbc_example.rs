#![expect(non_camel_case_types, non_snake_case)]
use std::{ffi::c_void, marker::PhantomData};

use co3::{Tag, ReprC, ffi, rust_spec::RustSpec, slice::Unpack2};

pub trait OdbcVersion {}

#[derive(ReprC)]
#[repr(transparent)]
pub struct OdbcStr<C>([C]);

type SQLSMALLINT = i16;
pub enum Payload {}

impl OdbcVersion for Payload {}

#[derive(Debug, RustSpec, Tag, ReprC)]
#[tag(SQLSMALLINT)]
#[repr(transparent)]
pub struct MyType<V: OdbcVersion = Payload> {
    pub(crate) handle: *mut c_void,
    _version: PhantomData<V>,
}

pub enum SQL_OV_ODBC3 {}
pub enum SQL_OV_ODBC3_80 {}
pub enum SQL_OV_ODBC4 {}

impl OdbcVersion for SQL_OV_ODBC3 {}
impl OdbcVersion for SQL_OV_ODBC3_80 {}
impl OdbcVersion for SQL_OV_ODBC4 {}

ffi! {
    #![unsafe(extern("system"))]

    #[tag(SQLSMALLINT, unsafe(8))]
    type Opaque<V: OdbcVersion = SQL_OV_ODBC3_80>;

    impl<V: OdbcVersion> Drop for dyn Opaque<V>
    where
        use<V> @ (
            <SQL_OV_ODBC3> |
            <SQL_OV_ODBC3_80>
        )
    {
        fn drop(&mut self) -> SQLSMALLINT;
    }
}

trait Allocate {
    type Source;
}

impl<V: OdbcVersion> Allocate for SQLHENV<V> {
    type Source = i32;
}

impl<V: OdbcVersion> Allocate for SQLHDESC<u32, V> {
    type Source = u32;
}

co3::ffi! {
    #![unsafe(extern("system"))]

    #[tag(SQLSMALLINT, unsafe(8))]
    type SQLHENV<V: OdbcVersion = SQL_OV_ODBC3_80>;

    #[tag(SQLSMALLINT, unsafe(9))]
    type SQLHDESC<DT, V: OdbcVersion = SQL_OV_ODBC3_80>;

    impl<V: OdbcVersion, dyn(SQLSMALLINT) T> Drop for T
    where
        use<T> @ <SQLHENV<V>>
    {
        #[symbol_name = "SQLFreeHandle"]
        fn drop(&mut self) -> SQLSMALLINT;
    }

    impl<V: OdbcVersion, DT> Drop for SQLHDESC<DT, V> {
        #[symbol_name = "SQLFreeHandle"]
        fn drop(&mut self) -> SQLSMALLINT;
    }

    #[symbol_name = "SQLAllocHandle"]
    pub fn SQLAllocHandle<'a, 'b, dyn(SQLSMALLINT) T: Allocate, V: OdbcVersion>(
        InputHandle: &'a T::Source,
        OutputHandlePtr: &'b mut T,
    )
    where
        use<T> @ (<OwnedSQLHENV<V>> | <OwnedSQLHDESC<u32, V>>);

    impl<V: OdbcVersion> SQLHENV<V> {
        pub fn get_attr0(&self);
        pub fn get_attr1(a: &Self);
        pub fn get_attr2(one: u32, &self);
        pub fn get_attr3<dyn(u32) T = u32>(one: T, &self)
        where
            use<T> @ <SQLHENV2>;
    }

    impl<V: OdbcVersion> SQLHENV<V> {
        /// Returns an environment attribute through the generated public wrapper.
        ///
        /// This placement intentionally precedes `symbol_name`.
        #[symbol_name = "SQLGetEnvAttr"]
        pub fn get_attr<dyn(i32) A: EnvAttr>(
            &self,
            attribute: <dyn A>::TAG,
            #[unpack(u32, i32)]
            move value1: <A as EnvAttr>::Value,
            #[unpack(u32, i32)]
            value2: <A as EnvAttr>::Value,
            string_length: &mut i32,
        )
        where
            use<A> @ (<SQL_ATTR_ODBC_VERSION> | <SQL_ATTR_CP_MATCH>);
    }
}

#[derive(Clone)]
struct SQLHENV2(u32);

impl SQLHENV2 {
    pub fn get_attr0(&self) {}
    pub fn get_attr1(_a: &Self) {}
    pub fn get_attr2(self: Box<Self>) {}
    pub fn get_attr3<T>(&self, _one: T) {}
}

co3::ffi! {
    #![unsafe(export("system"))]

    #[tag(u32, unsafe(7))]
    type SQLHENV2;

    impl<dyn(u32) T> Drop for T
    where
        use<T> @ <SQLHENV2>
    {
        fn drop(&mut self);
    }

    impl SQLHENV2 {
        fn get_attr0(&self);
        fn get_attr1(a: &Self);
        fn get_attr2(self: Box<Self>);
        fn get_attr3<dyn(u32) T = u32>(&self, move one: T)
        where
            use<T> @ <SQLHENV2>;
    }
}

pub trait EnvAttr {
    type Value;
}

#[derive(Clone, Tag)]
#[tag(i32, unsafe(80))]
enum SQL_ATTR_ODBC_VERSION {}

#[derive(Clone, Tag)]
#[tag(i32, unsafe(81))]
enum SQL_ATTR_CP_MATCH {}
impl EnvAttr for SQL_ATTR_ODBC_VERSION {
    type Value = CpMatch;
}

impl EnvAttr for SQL_ATTR_CP_MATCH {
    type Value = ConnectionPooling;
}

impl Unpack2<u32, i32> for CpMatch {
    type Error = core::convert::Infallible;
    fn unpack(value: Self::CType) -> Result<(u32, i32), Self::Error> {
        Ok((value.0, 0))
    }
}

impl Unpack2<u32, i32> for ConnectionPooling {
    type Error = core::convert::Infallible;
    fn unpack(value: Self::CType) -> Result<(u32, i32), Self::Error> {
        Ok((value.0, 0))
    }
}

#[derive(Clone, ReprC)]
#[repr(u32)]
pub enum CpMatch {
    A,
}

#[derive(Clone, ReprC)]
#[repr(u32)]
enum ConnectionPooling {
    A,
    B,
}

fn main() {}
