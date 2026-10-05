use proc_macro2::TokenStream;
use quote::quote;

use super::{ReprKind, is_type_parametrized, wide::last_field};

pub(super) fn expand(
    input: &syn::DeriveInput,
    repr: Option<&ReprKind>,
) -> syn::Result<TokenStream> {
    let syn::Data::Struct(data) = &input.data else {
        return Ok(quote! {});
    };
    if !matches!(repr, Some(ReprKind::Transparent)) || data.fields.len() != 1 {
        return Ok(quote! {});
    }

    let (field, _, _) = last_field(&data.fields).expect("one field was checked");
    let field_ty = &field.ty;
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let for_dummy =
        (!is_type_parametrized(field_ty, &input.generics)).then(|| quote! { for<'__dummy> });
    let alloc_method = cfg!(feature = "alloc").then(|| {
        quote! {
            fn into_non_null(
                self: co3::boxed::Box<Self>,
            ) -> core::ptr::NonNull<Self::Data> {
                let inner = unsafe {
                    co3::boxed::Box::from_raw(
                        co3::boxed::Box::into_raw(self) as *mut #field_ty
                    )
                };
                <#field_ty as co3::ffi::NulTerminatedBuf>::into_non_null(inner)
            }

            unsafe fn from_non_null(
                ptr: core::ptr::NonNull<Self::Data>,
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
            type Data = <#field_ty as co3::ffi::NulTerminatedBuf>::Data;

            fn as_ptr(ptr: *const Self) -> *const Self::Data {
                <#field_ty as co3::ffi::NulTerminatedBuf>::as_ptr(ptr as *const #field_ty)
            }

            unsafe fn from_raw<'a>(ptr: *const Self::Data) -> &'a Self {
                let inner = unsafe {
                    <#field_ty as co3::ffi::NulTerminatedBuf>::from_raw(ptr)
                };
                unsafe { &*(inner as *const #field_ty as *const Self) }
            }
            #alloc_method
        }
    })
}
