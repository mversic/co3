use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{Co3Static, co3_path, symbol_name_value, utils::cfg_attrs};

fn static_raw_ident(ident: &syn::Ident) -> syn::Ident {
    format_ident!("__co3_static_{ident}")
}

fn static_wrapper_ident(ident: &syn::Ident) -> syn::Ident {
    format_ident!("__Co3Static_{ident}")
}

fn gen_static_assert(ty: &syn::Type) -> TokenStream {
    let co3 = co3_path();

    quote! {
        const _: () = {
            assert!(
                #co3::impls!(#ty: #co3::rust_spec::RustSpec<
                    Layout = #co3::rust_spec::Stable,
                    Size = #co3::rust_spec::size::Sized<#co3::rust_spec::Gt<#co3::rust_spec::Zero>>
                >),
                concat!(
                    "ffi static `",
                    stringify!(#ty),
                    "` must have a stable, non-zero layout"
                )
            );
        };
    }
}

fn wrapper_definition(
    vis: &syn::Visibility,
    ident: &syn::Ident,
    ty: &syn::Type,
    mutable: bool,
) -> (syn::Ident, TokenStream, TokenStream) {
    let wrapper_ident = static_wrapper_ident(ident);

    let (definition, value) = if mutable {
        (
            quote! {
                #[derive(Clone, Copy)]
                #[doc(hidden)]
                #vis struct #wrapper_ident(
                    core::marker::PhantomData<core::cell::UnsafeCell<#ty>>,
                );
            },
            quote! {
                #vis const #ident: #wrapper_ident = #wrapper_ident(core::marker::PhantomData);
            },
        )
    } else {
        (
            quote! {
                #[derive(Clone, Copy)]
                #[doc(hidden)]
                #vis struct #wrapper_ident(core::marker::PhantomData<#ty>);
            },
            quote! {
                #vis const #ident: #wrapper_ident = #wrapper_ident(core::marker::PhantomData);
            },
        )
    };

    (wrapper_ident, definition, value)
}

fn gen_import_wrapper(
    vis: &syn::Visibility,
    ident: &syn::Ident,
    ty: &syn::Type,
    raw_ident: &syn::Ident,
    mutable: bool,
) -> TokenStream {
    let (wrapper_ident, definition, value) = wrapper_definition(vis, ident, ty, mutable);

    let co3 = co3_path();
    let decode_bounds = quote! {
        for<'_dummy> #ty: #co3::Decode<'_dummy, Store: #co3::stored::EmptyStore>,
        <#ty as #co3::ReprC>::CType: Copy,
    };

    let soft_read = if mutable {
        quote! {
            #[inline]
            pub unsafe fn soft_read<'__co3_static_d>(
                &self,
                store: &'__co3_static_d mut <#ty as #co3::stored::DecodeOwned<'__co3_static_d>>::Store,
            ) -> Option<#ty>
            where
                for<'_dummy> #ty: #co3::Decode<'__co3_static_d>,
                <#ty as #co3::ReprC>::CType: Copy,
            {
                let source = unsafe { core::ptr::read(core::ptr::addr_of!(#raw_ident)) };
                unsafe { #co3::soft_decode::<#ty>(source, store) }
            }
        }
    } else {
        quote! {
            #[inline]
            pub fn soft_read<'__co3_static_d>(
                &self,
                store: &'__co3_static_d mut <#ty as #co3::stored::DecodeOwned<'__co3_static_d>>::Store,
            ) -> Option<#ty>
            where
                for<'_dummy> #ty: #co3::Decode<'__co3_static_d>,
                <#ty as #co3::ReprC>::CType: Copy,
            {
                let source = unsafe { #raw_ident };
                unsafe { #co3::soft_decode::<#ty>(source, store) }
            }
        }
    };

    let read = if mutable {
        quote! {
            #[inline]
            pub unsafe fn read(&self) -> Option<#ty>
            where #decode_bounds
            {
                let source = unsafe { core::ptr::read(core::ptr::addr_of!(#raw_ident)) };
                unsafe { #co3::decode::<#ty>(source) }
            }
        }
    } else {
        quote! {
            #[inline]
            pub fn read(&self) -> Option<#ty>
            where #decode_bounds
            {
                let source = unsafe { #raw_ident };
                unsafe { #co3::decode::<#ty>(source) }
            }
        }
    };

    let direct_access = (!mutable).then(|| {
        quote! {
            #[inline]
            pub fn get(&self) -> Option<&#ty>
            where
                for<'_dummy> #ty: #co3::transmute::CheckedTransmute + Sync,
            {
                let valid = unsafe {
                    <#ty as #co3::transmute::CheckedTransmute>::is_valid(
                        unsafe { &#raw_ident },
                    )
                };
                valid.then(|| unsafe {
                    &*(&#raw_ident as *const _ as *const #ty)
                })
            }

            #[inline]
            pub unsafe fn get_unchecked(&self) -> &#ty
            where for<'_dummy> #ty: #co3::transmute::CheckedTransmute,
            {
                unsafe { &*(&#raw_ident as *const _ as *const #ty) }
            }
        }
    });

    let mutable_methods = mutable.then(|| quote! {
        #[inline]
        pub unsafe fn set(&self, value: #ty)
        where for<'_dummy> #ty: #co3::Encode<Store: #co3::stored::EmptyStore>,
        {
            let value = #co3::encode(value);
            unsafe { core::ptr::write(core::ptr::addr_of_mut!(#raw_ident), value); }
        }

        #[inline]
        pub unsafe fn take(&self) -> Option<#ty>
        where
            for<'_dummy> #ty: #co3::Encode<Store: #co3::stored::EmptyStore> + core::default::Default,
            #decode_bounds
        {
            let replacement = #co3::encode(<#ty as core::default::Default>::default());
            let old = unsafe { core::ptr::replace(core::ptr::addr_of_mut!(#raw_ident), replacement) };
            unsafe { #co3::decode::<#ty>(old) }
        }
    });

    quote! {
        #definition
        impl #wrapper_ident {
            #direct_access
            #soft_read
            #read
            #mutable_methods
        }
        #value
    }
}

pub(crate) fn gen_extern_static(
    abi: &syn::Abi,
    block_attrs: &[syn::Attribute],
    item: Co3Static,
) -> TokenStream {
    let co3 = co3_path();

    let Co3Static {
        attrs,
        vis,
        static_token,
        mutability,
        ident,
        ty,
        ..
    } = item;

    let raw_ident = static_raw_ident(&ident);
    let cfg_attrs = cfg_attrs(&attrs)
        .map(|attr| quote!(#attr))
        .collect::<Vec<_>>();

    let link_attrs = attrs
        .iter()
        .filter_map(|attr| symbol_name_value(attr).map(|value| quote!(#[link_name = #value])));

    let attrs = attrs
        .iter()
        .filter(|attr| symbol_name_value(attr).is_none());

    let static_assert = gen_static_assert(&ty);
    let mutable = !matches!(mutability, syn::StaticMutability::None);
    let wrapper = gen_import_wrapper(&vis, &ident, &ty, &raw_ident, mutable);

    quote! {
        #(#cfg_attrs)*
        #static_assert

        #(#cfg_attrs)*
        #wrapper

        #(#cfg_attrs)*
        unsafe #abi {
            #(#block_attrs)*

            #(#attrs)*
            #(#link_attrs)*
            #static_token #mutability #raw_ident: <#ty as #co3::ReprC>::CType;
        }
    }
}
