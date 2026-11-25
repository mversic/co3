use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_quote, visit_mut::VisitMut};

use super::{
    ReprKind,
    ctype::{TailClass, UnsizedTail, classify_tail, gen_fields_methods},
    is_parametrized,
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

pub(super) fn wide_tail_type<'a>(
    fields: &'a syn::Fields,
    repr: Option<&ReprKind>,
    generics: &syn::Generics,
) -> Option<&'a syn::Type> {
    let tail = &fields.iter().next_back()?.ty;
    if fields.len() == 1 && !matches!(repr, Some(ReprKind::Transparent)) {
        return None;
    }
    if matches!(
        classify_tail(tail, generics),
        TailClass::Sized | TailClass::Unsized(UnsizedTail::NulTerminated)
    ) {
        return None;
    }
    Some(tail)
}

pub(crate) fn expand(
    input: &syn::DeriveInput,
    repr: &super::attr::Repr,
    recursive_header: Option<&super::inference::RecursiveFields>,
) -> syn::Result<TokenStream> {
    let kind = repr.kind.as_ref();
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);

    let syn::Data::Struct(data) = &input.data else {
        return Ok(quote! {});
    };

    let Some(field_ty) = wide_tail_type(&data.fields, kind, &input.generics) else {
        return Ok(quote! {});
    };

    let name = &input.ident;
    let is_transparent = matches!(kind, Some(ReprKind::Transparent));

    let header_ty = if is_transparent {
        quote!(<#field_ty as co3::wide::Wide>::Header)
    } else {
        gen_data_struct_ty(input)
    };
    let methods = gen_dst_methods(is_transparent, field_ty);
    let alloc_methods = gen_alloc_methods(is_transparent, field_ty);

    let header_predicate = header_repr_c_predicate(field_ty, &input.generics);
    let usize_predicate = (!is_transparent).then(|| {
        let predicate = wide_usize_predicate(field_ty, &input.generics);
        quote!(#predicate,)
    });
    let data_def =
        (!is_transparent).then(|| gen_data_def(input, &data.fields, repr, recursive_header));

    Ok(quote! {
        #data_def
        unsafe impl #impl_generics co3::wide::Wide for #name #ty_generics
        where
            #header_predicate,
            #usize_predicate
            #predicates
        {
            type Header = #header_ty;
            type Metadata = <#field_ty as co3::wide::Wide>::Metadata;

            #methods
            #alloc_methods
        }
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

fn gen_data_struct_ty(input: &syn::DeriveInput) -> TokenStream {
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

fn gen_data_def(
    input: &syn::DeriveInput,
    fields: &syn::Fields,
    repr: &super::attr::Repr,
    recursive_header: Option<&super::inference::RecursiveFields>,
) -> TokenStream {
    let mut header = input.clone();
    header.ident = gen_data_struct_name(&input.ident);
    header
        .attrs
        .retain(|attr| attr.path().is_ident("repr") || attr.path().is_ident("cfg"));
    header.generics = data_generics(fields, &input.generics);
    // In a header field, Self still denotes the original, potentially wide record.
    let name = &input.ident;
    let (_, args, _) = input.generics.split_for_impl();
    let own = parse_quote!(#name #args);
    crate::utils::ConcreteSelfType(&own).visit_derive_input_mut(&mut header);

    let syn::Data::Struct(data) = &mut header.data else {
        unreachable!()
    };
    for f in &mut data.fields {
        f.vis = syn::Visibility::Inherited;
        f.attrs
            .retain(|attr| attr.path().is_ident("cfg") || attr.path().is_ident("cfg_attr"));
    }
    let tail = data
        .fields
        .iter_mut()
        .next_back()
        .expect("wide record has a tail");
    tail.ty = if recursive_header.is_some()
        && let syn::Type::Slice(s) = &tail.ty
    {
        let elem = &s.elem;
        parse_quote!([#elem; 0])
    } else {
        data_field_ty(&tail.ty, true)
    };

    let fields_methods =
        gen_fields_methods(&header.ident, &header.generics, &data.fields, false, false);
    let header_impl = gen_identity_header_impl(&header.ident, &header.generics);
    let derives = regular_derives(input);
    let mut header_input = header.clone();
    if let Some(recursion) = recursive_header {
        let predicates = &mut header_input.generics.make_where_clause().predicates;
        predicates.extend(recursion.tail_bounds.iter().cloned());
        predicates.extend(super::ctype::projected_field_bounds(
            &input.generics,
            &recursion.types(),
            &recursion.carriers,
        ));
    }
    let recursion = super::inference::RecursiveFields::analyze(
        &header_input,
        repr.kind.as_ref(),
        recursive_header.map(|r| r.carriers.as_slice()),
    );
    let attrs = super::ReprCAttrs {
        is_wide_data: true,
        ..Default::default()
    };
    let impls = super::item::derive_item(
        repr.kind.as_ref(),
        repr.alignment.as_ref(),
        &header_input,
        &attrs,
        &[],
        &recursion,
    );
    quote! {
        #[derive(#(#derives, )* co3::rust_spec::RustSpec)] #header
        #impls #fields_methods #header_impl
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
        .push(wide_predicate);
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

            let modules: &[&str] = match name.as_str() {
                "Clone" => &["core", "std", "core::clone", "std::clone"],
                "Copy" => &["core", "std", "core::marker", "std::marker"],
                "Debug" => &["core", "std", "core::fmt", "std::fmt"],
                "Default" => &["core", "std", "core::default", "std::default"],
                "Hash" => &["core", "std", "core::hash", "std::hash"],
                _ => &["core", "std", "core::cmp", "std::cmp"],
            };
            if SUPPORTED.contains(&name.as_str())
                && crate::utils::builtin_path(&meta.path, &name, modules)
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

pub(super) fn wide_predicate(
    field_ty: &syn::Type,
    generics: &syn::Generics,
) -> syn::WherePredicate {
    let for_dummy = (!is_parametrized(field_ty, generics)).then(|| quote! { for<'__dummy> });
    parse_quote! { #for_dummy #field_ty: co3::wide::Wide }
}

pub(super) fn wide_usize_predicate(
    field_ty: &syn::Type,
    generics: &syn::Generics,
) -> syn::WherePredicate {
    let for_dummy = (!is_parametrized(field_ty, generics)).then(|| quote! { for<'__dummy> });

    parse_quote! {
        #for_dummy #field_ty: co3::wide::Wide<Metadata = usize> + co3::rust_spec::RustSpec<
            Size = co3::rust_spec::size::MetaSized<co3::rust_spec::size::SliceLike>
        >
    }
}

pub(super) fn header_repr_c_predicate(
    field_ty: &syn::Type,
    generics: &syn::Generics,
) -> syn::WherePredicate {
    let for_dummy = (!is_parametrized(field_ty, generics)).then(|| quote! { for<'__dummy> });
    parse_quote! { #for_dummy #field_ty: co3::wide::Wide<Header: co3::ReprC<CType: Sized>> }
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
                let raw = core::ptr::slice_from_raw_parts(data.cast::<#tail_data_ty>(), metadata);
                const { assert!(core::mem::size_of::<*const Self>() == core::mem::size_of::<*const [#tail_data_ty]>()); }
                let ptr: *const Self = unsafe { core::mem::transmute_copy(&raw) };
                unsafe { &*ptr }
            }

            #[inline(always)]
            unsafe fn from_raw_parts_mut<'__rust_spec>(
                data: *mut Self::Header,
                metadata: Self::Metadata,
            ) -> &'__rust_spec mut Self {
                let raw = core::ptr::slice_from_raw_parts_mut(data.cast::<#tail_data_ty>(), metadata);
                const { assert!(core::mem::size_of::<*mut Self>() == core::mem::size_of::<*mut [#tail_data_ty]>()); }
                let ptr: *mut Self = unsafe { core::mem::transmute_copy(&raw) };
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
            let inner = unsafe {
                <#field_ty as co3::wide::Wide>::from_non_null(data.cast(), metadata)
            };
            let ptr = co3::boxed::Box::into_raw(inner) as *mut Self;
        }
    } else {
        quote! {
            let raw = core::ptr::slice_from_raw_parts_mut(data.as_ptr().cast::<#tail_data_ty>(), metadata);
            const { assert!(core::mem::size_of::<*mut Self>() == core::mem::size_of::<*mut [#tail_data_ty]>()); }
            let ptr: *mut Self = unsafe { core::mem::transmute_copy(&raw) };
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
