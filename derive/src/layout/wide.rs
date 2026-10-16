use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse_quote;

use super::{
    ReprKind,
    ctype::{WideTail, gen_fields_methods, wide_tail},
    generic_param_idents, is_parametrized, is_phantom_data,
};

pub(super) fn gen_data_struct_name(ident: &syn::Ident) -> syn::Ident {
    format_ident!("{}Data", ident)
}

pub(super) fn gen_identity_header_impl(name: &syn::Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    quote! {
        impl #impl_generics co3::wide::WideHeader for #name #ty_generics #where_clause {
            type Data = Self;
        }
    }
}

pub(crate) fn expand(
    input: &syn::DeriveInput,
    repr: Option<&ReprKind>,
) -> syn::Result<TokenStream> {
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let syn::Data::Struct(data) = &input.data else {
        return Ok(quote! {});
    };

    let Some((field, ..)) = last_field(&data.fields) else {
        return Ok(quote! {});
    };

    if is_phantom_data(&field.ty) {
        return Ok(quote! {});
    }

    if wide_tail(&field.ty, &input.generics) == WideTail::No {
        return Ok(quote! {});
    }

    let name = &input.ident;
    let field_ty = &field.ty;

    let is_single = data.fields.len() == 1;
    let is_transparent_single = is_single && matches!(repr, Some(ReprKind::Transparent));
    let generates_wide = !is_single || is_transparent_single;
    let data_ty = if is_transparent_single {
        quote!(<#field_ty as co3::wide::Wide>::Header)
    } else {
        gen_data_ty(input)
    };
    let methods = gen_dst_methods(is_transparent_single, field_ty);
    let alloc_methods = gen_alloc_methods(is_transparent_single, field_ty);

    let header_predicate = header_repr_c_predicate(field_ty, &input.generics);
    let usize_predicate = (!is_transparent_single).then(|| {
        let predicate = wide_usize_predicate(field_ty, &input.generics);
        quote!(#predicate,)
    });
    let data_def =
        (!is_transparent_single && generates_wide).then(|| gen_data_def(input, &data.fields));
    let wide_impl = generates_wide.then(|| {
        quote! {
            unsafe impl #impl_generics co3::wide::Wide for #name #ty_generics
            where
                #header_predicate,
                #usize_predicate
                #predicates
            {
                type Header = #data_ty;
                type Metadata = <#field_ty as co3::wide::Wide>::Metadata;

                #methods
                #alloc_methods
            }
        }
    });

    Ok(quote! {
        #data_def
        #wide_impl
    })
}

pub(super) fn last_field(fields: &syn::Fields) -> Option<(&syn::Field, TokenStream, TokenStream)> {
    let (index, field) = fields.iter().enumerate().next_back()?;

    let (field_ref, field_member) = field.ident.as_ref().map_or_else(
        || {
            let index = syn::Index::from(index);
            (quote! { self.#index }, quote! { #index })
        },
        |field_name| (quote! { self.#field_name }, quote! { #field_name }),
    );

    Some((field, field_ref, field_member))
}

fn gen_data_ty(input: &syn::DeriveInput) -> TokenStream {
    let name = gen_data_struct_name(&input.ident);

    let args = input.generics.params.iter().map(|param| match param {
        syn::GenericParam::Lifetime(param) => {
            let lifetime = &param.lifetime;
            quote! { #lifetime }
        }
        syn::GenericParam::Type(param) => {
            let ident = &param.ident;
            quote! { #ident }
        }
        syn::GenericParam::Const(param) => {
            let ident = &param.ident;
            quote! { #ident }
        }
    });

    quote! { #name <#(#args),*> }
}

fn gen_data_def(input: &syn::DeriveInput, fields: &syn::Fields) -> TokenStream {
    if let Some((last, ..)) = last_field(fields)
        && wide_tail(&last.ty, &input.generics) == WideTail::Unknown
        && !is_parametrized(&last.ty, &input.generics)
    {
        let tail_ty = &last.ty;
        let tail_param = format_ident!("__Co3WideTail");
        let mut internal_input = input.clone();
        internal_input.ident = format_ident!("__Co3{}", input.ident);
        internal_input
            .generics
            .params
            .push(parse_quote!(#tail_param: ?Sized));
        let mut internal_fields = fields.clone();
        internal_fields
            .iter_mut()
            .next_back()
            .expect("wide data has a tail")
            .ty = parse_quote!(#tail_param);
        let internal_def = gen_data_def(&internal_input, &internal_fields);

        let name = gen_data_struct_name(&input.ident);
        let internal_name = gen_data_struct_name(&internal_input.ident);
        let ctype_name = format_ident!("C{name}");
        let internal_ctype_name = format_ident!("C{internal_name}");
        let mut alias_generics = input.generics.clone();
        alias_generics
            .params
            .push(parse_quote!(#tail_param: ?Sized = #tail_ty));
        let args = generic_param_idents(alias_generics.params.iter()).collect::<Vec<_>>();
        let vis = &input.vis;

        return quote! {
            #internal_def
            #vis type #name #alias_generics = #internal_name<#(#args),*>;
            #vis type #ctype_name #alias_generics = #internal_ctype_name<#(#args),*>;
        };
    }

    let name = gen_data_struct_name(&input.ident);

    let attrs = input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("repr") || attr.path().is_ident("cfg"));
    let derives = regular_derives(input);

    let vis = &input.vis;
    let fields_len = fields.len();

    let generics = data_generics(fields, &input.generics);
    let (impl_generics, _, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let header_impl = gen_identity_header_impl(&name, &generics);
    let mut data_fields = fields.clone();
    for (index, field) in data_fields.iter_mut().enumerate() {
        field.ty = data_field_ty(&field.ty, index == fields_len - 1);
        field.vis = syn::Visibility::Inherited;
    }
    let fields_methods = gen_fields_methods(&name, &generics, &data_fields, false);

    let suffix =
        match fields {
            syn::Fields::Named(fields) => {
                let fields = fields.named.iter().enumerate().map(|(index, field)| {
                    let attrs = field.attrs.iter().filter(|attr| {
                        attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr")
                    });
                    let name = &field.ident;

                    let is_last_field = index == fields_len - 1;
                    let ty = data_field_ty(&field.ty, is_last_field);

                    quote! { #(#attrs)* #name: #ty }
                });

                quote! {
                    where #predicates
                    { #(#fields,)* }
                }
            }
            syn::Fields::Unnamed(fields) => {
                let fields = fields.unnamed.iter().enumerate().map(|(index, field)| {
                    let attrs = field.attrs.iter().filter(|attr| {
                        attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr")
                    });
                    let is_last_field = index == fields_len - 1;
                    let ty = data_field_ty(&field.ty, is_last_field);

                    quote! { #(#attrs)* #ty }
                });

                quote! {
                    ( #(#fields,)*)
                    where #predicates;
                }
            }
            syn::Fields::Unit => quote! {;},
        };

    quote! {
        #[derive(#(#derives, )* co3::rust_spec::RustSpec, co3::ReprC)]
        #[repr_c(__wide_data)]
        #(#attrs)*
        #vis struct #name #impl_generics #suffix

        #fields_methods
        #header_impl
    }
}

pub(super) fn data_generics(fields: &syn::Fields, generics: &syn::Generics) -> syn::Generics {
    let Some((last, ..)) = last_field(fields) else {
        return generics.clone();
    };

    let mut data_generics = generics.clone();
    let wide_predicate = wide_predicate(&last.ty, generics);
    data_generics
        .make_where_clause()
        .predicates
        .push(parse_quote!(#wide_predicate));
    data_generics
}

fn regular_derives(input: &syn::DeriveInput) -> Vec<syn::Path> {
    const SUPPORTED: &[&str] = &[
        "Clone",
        "Copy",
        "Debug",
        "Default",
        "Eq",
        "Hash",
        "Ord",
        "PartialEq",
        "PartialOrd",
    ];

    let mut derives = Vec::new();
    for attr in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
    {
        let _ = attr.parse_nested_meta(|meta| {
            let Some(name) = meta
                .path
                .segments
                .last()
                .map(|segment| segment.ident.to_string())
            else {
                return Ok(());
            };

            if SUPPORTED.contains(&name.as_str())
                && !derives.iter().any(|path: &syn::Path| {
                    path.segments.last().map(|segment| &segment.ident)
                        == meta.path.segments.last().map(|segment| &segment.ident)
                })
            {
                derives.push(meta.path.clone());
            }
            Ok(())
        });
    }
    derives
}

fn data_field_ty(ty: &syn::Type, is_last_field: bool) -> syn::Type {
    if !is_last_field {
        return ty.clone();
    }

    parse_quote! { <#ty as co3::wide::Wide>::Header }
}

pub(super) fn data_bound_ty(ty: &syn::Type, is_last_field: bool) -> syn::Type {
    if is_last_field {
        parse_quote!(<<#ty as co3::wide::Wide>::Header as co3::wide::WideHeader>::Data)
    } else {
        data_field_ty(ty, is_last_field)
    }
}

pub(super) fn wide_predicate(field_ty: &syn::Type, generics: &syn::Generics) -> TokenStream {
    let for_dummy = (!is_parametrized(field_ty, generics)).then(|| quote! { for<'__dummy> });
    quote! { #for_dummy #field_ty: co3::wide::Wide }
}

pub(super) fn wide_usize_predicate(field_ty: &syn::Type, generics: &syn::Generics) -> TokenStream {
    let for_dummy = (!is_parametrized(field_ty, generics)).then(|| quote! { for<'__dummy> });
    quote! { #for_dummy #field_ty: co3::wide::Wide<Metadata = usize> }
}

pub(super) fn header_repr_c_predicate(
    field_ty: &syn::Type,
    generics: &syn::Generics,
) -> TokenStream {
    let for_dummy = (!is_parametrized(field_ty, generics)).then(|| quote! { for<'__dummy> });
    quote! { #for_dummy #field_ty: co3::wide::Wide<Header: co3::ReprC<CType: Sized>> }
}

pub(super) fn gen_dst_methods(is_transparent: bool, field_ty: &syn::Type) -> TokenStream {
    let tail_data_ty = data_bound_ty(field_ty, true);
    let constructors = if is_transparent {
        quote! {
            #[inline(always)]
            unsafe fn from_raw_parts<'__rust_spec>(
                data: *const Self::Header,
                metadata: Self::Metadata,
            ) -> &'__rust_spec Self {
                let inner = unsafe { <#field_ty as co3::wide::Wide>::from_raw_parts(data.cast(), metadata) };
                unsafe { &*(inner as *const #field_ty as *const Self) }
            }

            #[inline(always)]
            unsafe fn from_raw_parts_mut<'__rust_spec>(
                data: *mut Self::Header,
                metadata: Self::Metadata,
            ) -> &'__rust_spec mut Self {
                let inner = unsafe { <#field_ty as co3::wide::Wide>::from_raw_parts_mut(data.cast(), metadata) };
                unsafe { &mut *(inner as *mut #field_ty as *mut Self) }
            }
        }
    } else {
        quote! {
            #[inline(always)]
            unsafe fn from_raw_parts<'__rust_spec>(
                data: *const Self::Header,
                metadata: Self::Metadata,
            ) -> &'__rust_spec Self {
                let ptr = core::ptr::slice_from_raw_parts(data.cast::<#tail_data_ty>(), metadata) as *const Self;
                unsafe { &*ptr }
            }

            #[inline(always)]
            unsafe fn from_raw_parts_mut<'__rust_spec>(
                data: *mut Self::Header,
                metadata: Self::Metadata,
            ) -> &'__rust_spec mut Self {
                let ptr = core::ptr::slice_from_raw_parts_mut(data.cast::<#tail_data_ty>(), metadata) as *mut Self;
                unsafe { &mut *ptr }
            }
        }
    };

    quote! {
        #[inline(always)]
        fn metadata(ptr: *const Self) -> Self::Metadata {
            <#field_ty as co3::wide::Wide>::metadata(ptr as *const #field_ty)
        }

        #[inline(always)]
        fn as_ptr(ptr: *const Self) -> *const Self::Header {
            ptr as *const Self::Header
        }

        #[inline(always)]
        fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Header {
            ptr as *mut Self::Header
        }

        #constructors
    }
}

pub(super) fn gen_alloc_methods(is_transparent: bool, field_ty: &syn::Type) -> TokenStream {
    if !cfg!(feature = "alloc") {
        return quote! {};
    }

    let tail_data_ty = data_bound_ty(field_ty, true);
    let constructor = if is_transparent {
        quote! {
            let ptr = unsafe { <Self as co3::wide::Wide>::from_raw_parts_mut(data.as_ptr(), metadata) }
                as *mut Self;
        }
    } else {
        quote! {
            let ptr = core::ptr::slice_from_raw_parts_mut(data.as_ptr().cast::<#tail_data_ty>(), metadata)
                as *mut Self;
        }
    };

    quote! {
        #[inline(always)]
        fn into_non_null(self: co3::boxed::Box<Self>) -> core::ptr::NonNull<Self::Header> {
            co3::boxed::Box::into_non_null(self).cast()
        }

        #[inline(always)]
        unsafe fn from_non_null(
            data: core::ptr::NonNull<Self::Header>,
            metadata: Self::Metadata,
        ) -> co3::boxed::Box<Self> {
            #constructor
            unsafe { co3::boxed::Box::from_raw(ptr) }
        }
    }
}
