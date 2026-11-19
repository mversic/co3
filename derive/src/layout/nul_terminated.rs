use proc_macro2::TokenStream;
use quote::quote;

use super::{
    ReprKind,
    ctype::{TailClass, UnsizedTail, classify_tail},
    is_parametrized,
    wide::last_field,
};

pub(super) fn expand(
    input: &syn::DeriveInput,
    repr: Option<&ReprKind>,
) -> syn::Result<TokenStream> {
    let syn::Data::Struct(data) = &input.data else {
        return Ok(quote! {});
    };
    if !matches!(repr, Some(ReprKind::Transparent)) {
        return Ok(quote! {});
    }

    let Some((field, _, field_member)) = last_field(&data.fields) else {
        return Ok(quote! {});
    };
    if matches!(
        classify_tail(&field.ty, &input.generics),
        TailClass::Sized | TailClass::Unsized(UnsizedTail::SliceLike)
    ) {
        return Ok(quote! {});
    }
    let field_ty = &field.ty;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let for_dummy = (!is_parametrized(field_ty, &input.generics)).then(|| quote! { for<'__dummy> });
    let alloc_method = cfg!(feature = "alloc").then(|| {
        quote! {
            fn into_non_null(
                self: co3::boxed::Box<Self>,
            ) -> core::ptr::NonNull<Self::Header> {
                let inner = unsafe {
                    co3::boxed::Box::from_raw(
                        co3::boxed::Box::into_raw(self) as *mut #field_ty
                    )
                };
                <#field_ty as co3::ffi::NulTerminatedBuf>::into_non_null(inner)
            }

            unsafe fn from_non_null(
                ptr: core::ptr::NonNull<Self::Header>,
            ) -> co3::boxed::Box<Self> {
                let inner = unsafe {
                    <#field_ty as co3::ffi::NulTerminatedBuf>::from_non_null(ptr.cast())
                };
                unsafe {
                    co3::boxed::Box::from_raw(
                        co3::boxed::Box::into_raw(inner) as *mut Self
                    )
                }
            }
        }
    });

    Ok(quote! {
        unsafe impl #impl_generics co3::ffi::NulTerminatedBuf for #name #ty_generics
        where
            #for_dummy #field_ty: co3::ffi::NulTerminatedBuf,
            #predicates
        {
            type Header = <#field_ty as co3::ffi::NulTerminatedBuf>::Header;

            fn as_ptr(value: &Self) -> *const Self::Header {
                <#field_ty as co3::ffi::NulTerminatedBuf>::as_ptr(&value.#field_member)
            }

            unsafe fn from_raw<'__co3_nul_terminated>(ptr: *const Self::Header) -> &'__co3_nul_terminated Self {
                let inner = unsafe {
                    <#field_ty as co3::ffi::NulTerminatedBuf>::from_raw(ptr)
                };
                unsafe { &*(inner as *const #field_ty as *const Self) }
            }
            #alloc_method
        }
    })
}
