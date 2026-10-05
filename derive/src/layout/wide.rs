use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse_quote;

use super::{
    ReprKind, ctype::gen_extern_c_bounds_for_ctype, is_phantom_data, is_type_parametrized,
};

pub(super) fn gen_data_struct_name(ident: &syn::Ident) -> syn::Ident {
    format_ident!("{}Data", ident)
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

    let Some((field, _, field_member)) = last_field(&data.fields) else {
        return Ok(quote! {});
    };

    if is_phantom_data(&field.ty) {
        return Ok(quote! {});
    }

    let name = &input.ident;
    let field_ty = &field.ty;

    let is_single = data.fields.len() == 1;
    let is_transparent_single = is_single && matches!(repr, Some(ReprKind::Transparent));
    let generates_wide = !is_single || is_transparent_single;
    let data_ty = if is_transparent_single {
        quote!(<#field_ty as co3::wide::Wide>::Data)
    } else {
        gen_data_ty(input)
    };
    let methods = gen_dst_methods(is_transparent_single, field_ty, field_member);
    let alloc_methods = gen_alloc_methods();

    let wide_predicate = wide_predicate(field_ty, &input.generics);
    let data_def =
        (!is_transparent_single && generates_wide).then(|| gen_data_def(input, &data.fields));
    let wide_impl = generates_wide.then(|| {
        quote! {
            unsafe impl #impl_generics co3::wide::Wide for #name #ty_generics
            where
                #wide_predicate,
                #predicates
            {
                type Data = #data_ty;
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

    let suffix =
        match fields {
            syn::Fields::Named(fields) => {
                let fields = fields.named.iter().enumerate().map(|(index, field)| {
                    let attrs = field.attrs.iter().filter(|attr| {
                        attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr")
                    });
                    let vis = &field.vis;
                    let name = &field.ident;

                    let is_last_field = index == fields_len - 1;
                    let ty = data_field_ty(&field.ty, is_last_field);

                    quote! { #(#attrs)* #vis #name: #ty }
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
                    let vis = &field.vis;

                    let is_last_field = index == fields_len - 1;
                    let ty = data_field_ty(&field.ty, is_last_field);

                    quote! { #(#attrs)* #vis #ty }
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

    match ty {
        syn::Type::Slice(_) => parse_quote! { [<#ty as co3::wide::Wide>::Data; 0] },
        _ => parse_quote! { <#ty as co3::wide::Wide>::Data },
    }
}

pub(super) fn data_bound_ty(ty: &syn::Type, is_last_field: bool) -> syn::Type {
    if is_last_field && matches!(ty, syn::Type::Slice(_)) {
        parse_quote!(<#ty as co3::wide::Wide>::Data)
    } else {
        data_field_ty(ty, is_last_field)
    }
}

pub(super) fn gen_data_ctype_bounds(
    fields: &syn::Fields,
    generics: &syn::Generics,
) -> Vec<syn::WherePredicate> {
    let fields_len = fields.len();

    let field_tys = fields
        .iter()
        .enumerate()
        .map(|(index, field)| data_bound_ty(&field.ty, index == fields_len - 1))
        .collect::<Vec<_>>();

    let generics = data_generics(fields, generics);
    let field_tys = field_tys.iter().collect::<Vec<_>>();
    gen_extern_c_bounds_for_ctype::<false>(&generics, &field_tys)
}

pub(super) fn wide_predicate(field_ty: &syn::Type, generics: &syn::Generics) -> TokenStream {
    let for_dummy = (!is_type_parametrized(field_ty, generics)).then(|| quote! { for<'__dummy> });
    quote! { #for_dummy #field_ty: co3::wide::Wide }
}

pub(super) fn gen_dst_methods(
    is_transparent: bool,
    field_ty: &syn::Type,
    field_member: TokenStream,
) -> TokenStream {
    let offset = if is_transparent {
        quote!(0)
    } else {
        quote!(core::mem::offset_of!(Self::Data, #field_member))
    };

    quote! {
        #[inline(always)]
        fn metadata(ptr: *const Self) -> Self::Metadata {
            <#field_ty as co3::wide::Wide>::metadata(ptr as *const #field_ty)
        }

        #[inline(always)]
        fn as_ptr(ptr: *const Self) -> *const Self::Data {
            ptr as *const Self::Data
        }

        #[inline(always)]
        fn as_mut_ptr(ptr: *mut Self) -> *mut Self::Data {
            ptr as *mut Self::Data
        }

        #[inline(always)]
        unsafe fn from_raw_parts<'__rust_spec>(
            data: *const Self::Data,
            metadata: Self::Metadata,
        ) -> &'__rust_spec Self {
            let offset = #offset;
            let field = unsafe {
                <#field_ty as co3::wide::Wide>::from_raw_parts(
                    data.cast::<u8>().byte_add(offset).cast(),
                    metadata,
                )
            };
            let field_ptr = field as *const #field_ty;
            let ptr = unsafe { (field_ptr as *const Self).byte_sub(offset) };

            unsafe { &*ptr }
        }

        #[inline(always)]
        unsafe fn from_raw_parts_mut<'__rust_spec>(
            data: *mut Self::Data,
            metadata: Self::Metadata,
        ) -> &'__rust_spec mut Self {
            let offset = #offset;
            let field = unsafe {
                <#field_ty as co3::wide::Wide>::from_raw_parts_mut(
                    data.cast::<u8>().byte_add(offset).cast(),
                    metadata,
                )
            };
            let field_ptr = field as *mut #field_ty;
            let ptr = unsafe { (field_ptr as *mut Self).byte_sub(offset) };

            unsafe { &mut *ptr }
        }
    }
}

pub(super) fn gen_alloc_methods() -> TokenStream {
    if !cfg!(feature = "alloc") {
        return quote! {};
    }

    quote! {
        #[inline(always)]
        fn into_non_null(self: co3::boxed::Box<Self>) -> core::ptr::NonNull<Self::Data> {
            co3::boxed::Box::into_non_null(self).cast()
        }

        #[inline(always)]
        unsafe fn from_non_null(
            data: core::ptr::NonNull<Self::Data>,
            metadata: Self::Metadata,
        ) -> co3::boxed::Box<Self> {
            let ptr = unsafe {
                <Self as co3::wide::Wide>::from_raw_parts_mut(data.as_ptr(), metadata)
            } as *mut Self;
            unsafe { co3::boxed::Box::from_raw(ptr) }
        }
    }
}
