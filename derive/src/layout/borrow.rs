use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Ident, parse_quote};

use crate::layout::{
    ReprCAttrs, VariantReprCAttrs,
    ctype::gen_ctype_name,
    generic_param_idents, is_phantom_data, is_type_parametrized,
    item::{field_vars, gen_fields_destructure},
};

pub(super) fn gen_item_view(
    input: &DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) -> TokenStream {
    if attrs.is_view {
        return quote! {};
    }

    if fields_are_only_phantom_data(&input.data) {
        return quote! {};
    }

    let mut view_def = input.clone();
    rewrite_view_attrs(&mut view_def, attrs, variant_attrs);

    view_def.ident = gen_view_name(&input.ident);

    rewrite_view_generics(&mut view_def);
    rewrite_view_fields(&mut view_def.data);

    let inherited_niche = attrs.niche_value.as_ref().map(|_| {
        let owner_name = &input.ident;
        let (_, owner_ty_generics, _) = input.generics.split_for_impl();
        quote! {
            #[repr_c(NICHE = co3::borrow::borrow_cast(
                <#owner_name #owner_ty_generics as co3::niche::Niche>::NICHE
            ))]
        }
    });

    quote! {
        #[derive(co3::rust_spec::RustSpec, co3::ReprC)]
        #[repr_c(view)]
        #inherited_niche
        #[doc(hidden)]
        #view_def
    }
}

pub(super) fn gen_item_borrow_impls(input: &DeriveInput) -> TokenStream {
    if fields_are_only_phantom_data(&input.data) {
        return gen_identity_borrow_impls(&input.ident, &input.generics);
    }

    match &input.data {
        syn::Data::Struct(data) => {
            gen_struct_borrow_impls(&input.ident, &input.generics, &data.fields)
        }
        syn::Data::Enum(data) => {
            gen_enum_borrow_impls(&input.ident, &input.generics, &data.variants)
        }
        syn::Data::Union(_) => unreachable!(),
    }
}

fn gen_struct_borrow_impls(
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
) -> TokenStream {
    let fields_destructure = gen_fields_destructure(fields);

    let view_name = gen_view_name(name);
    let field_tys = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let owner_type = struct_view_owner_type(fields);
    let borrowed_fields = gen_field_borrow_exprs(fields);
    let borrowed_view = gen_record_construction(quote! { #view_name }, fields, &borrowed_fields);
    let from_borrow_body = gen_record_from_borrow(quote!(Self), fields);

    let (borrow_impl, from_borrow_impl) = (
        quote! {
            let Self #fields_destructure = self;
            #borrowed_view
        },
        quote! {
            let #view_name #fields_destructure = source;
            #from_borrow_body
        },
    );

    gen_borrow_impls::<true>(
        name,
        generics,
        &field_tys,
        owner_type,
        borrow_impl,
        from_borrow_impl,
    )
}

fn gen_enum_borrow_impls(
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let view_name = gen_view_name(name);
    let owner_either_ty = gen_either_name(variants.len());

    let owner_type = enum_view_owner_type(variants);
    let (variants_borrow, variants_from_borrow): (Vec<_>, Vec<_>) = variants
        .iter()
        .enumerate()
        .map(|(idx, variant)| {
            let variant_name = &variant.ident;
            let field_vars = field_vars(&variant.fields);
            let destructure_fields = gen_fields_destructure(&variant.fields);
            let view_head = quote! { #view_name::#variant_name };
            let owned_head = quote! { Self::#variant_name };
            let borrowed_variant = gen_record_construction(view_head, &variant.fields, &field_vars);
            let from_borrow_body =
                gen_record_from_borrow(owned_head, &variant.fields);

            let owner_variant = either_variant_name(idx);
            let owner_vars = (0..field_vars.len())
                .map(|idx| format_ident!("__co3_owner_{idx}"))
                .collect::<Vec<_>>();
            let borrow_stmts = field_vars
                .iter()
                .zip(&variant.fields)
                .zip(&owner_vars)
                .filter(|((_, field), _)| {
                    !is_phantom_data(&field.ty)
                })
                .map(|((field_var, _), owner_var)| {
                        quote! {
                            let #field_var = co3::borrow::Borrow::borrow(#field_var, #owner_var);
                        }
                });

            (
                quote! {
                    Self::#variant_name #destructure_fields => {
                        let co3::either::#owner_either_ty::#owner_variant(owner) =
                            owner.insert(co3::either::#owner_either_ty::#owner_variant(Default::default()))
                        else {
                            unreachable!()
                        };

                        let (#(#owner_vars,)*) = owner;
                        #(#borrow_stmts)*
                        #borrowed_variant
                    }
                },
                quote! {
                    #view_name::#variant_name #destructure_fields => #from_borrow_body
                },
            )
        })
        .unzip();

    let (borrow_impl, from_borrow_impl) = (
        quote! { match self { #(#variants_borrow,)* } },
        quote! {
            match source { #(#variants_from_borrow,)* }
        },
    );

    gen_borrow_impls::<false>(
        name,
        generics,
        &fields,
        owner_type,
        borrow_impl,
        from_borrow_impl,
    )
}

fn gen_borrow_impls<const ADD_SIZED: bool>(
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    owner_type: TokenStream,
    borrow_impl: TokenStream,
    from_borrow_impl: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let mut from_borrow_generics = generics.clone();
    from_borrow_generics.params.insert(0, parse_quote!('_išč));
    let (from_borrow_impl_generics, _, _) = from_borrow_generics.split_for_impl();

    let mut view_ty_generics = Vec::new();
    if !fields.is_empty() {
        view_ty_generics.push(quote! { '_išč });
    }
    view_ty_generics.extend(generic_param_idents(&generics.params));
    let view_ty_generics = (!view_ty_generics.is_empty()).then(|| {
        quote! { <#(#view_ty_generics),*> }
    });
    let borrow_bounds = gen_field_trait_bounds(generics, fields, quote! { co3::borrow::Borrow });
    let from_borrow_bounds =
        gen_field_trait_bounds(generics, fields, quote! { co3::borrow::FromBorrow<'_išč> });

    let view_name = gen_view_name(name);
    let sized_bound = ADD_SIZED.then(|| {
        if generics.type_params().count() == 0 {
            quote! { for<'_dummy> Self: Sized, }
        } else {
            quote! { Self: Sized, }
        }
    });

    quote! {
        unsafe impl #impl_generics co3::borrow::Borrow for #name #ty_generics
        where
            #(#borrow_bounds,)*
            #sized_bound
            #predicates
        {
            type Borrowed<'_išč>
                = #view_name #view_ty_generics
            where
                Self: '_išč;

            type Owner = #owner_type;

            #[inline(always)]
            fn borrow<'_išč>(self, owner: &'_išč mut Self::Owner) -> Self::Borrowed<'_išč>
            where
                Self: '_išč,
            {
                #borrow_impl
            }
        }

        impl #from_borrow_impl_generics co3::borrow::FromBorrow<'_išč> for #name #ty_generics
        where
            #(#from_borrow_bounds,)*
            #sized_bound
            #predicates
        {
            #[inline(always)]
            fn from_borrow(source: Self::Borrowed<'_išč>) -> Self {
                #from_borrow_impl
            }
        }
    }
}

fn struct_view_owner_type(fields: &syn::Fields) -> TokenStream {
    let owners = fields.iter().map(|syn::Field { ty, .. }| {
        quote! { <#ty as co3::borrow::Borrow>::Owner }
    });

    quote!((#(#owners,)*))
}

fn enum_view_owner_type(
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
) -> TokenStream {
    let either_name = gen_either_name(variants.len());

    let owners = variants
        .iter()
        .map(|variant| struct_view_owner_type(&variant.fields));

    quote! { core::option::Option<co3::either::#either_name<#(#owners),*>> }
}

fn gen_field_borrow_exprs(fields: &syn::Fields) -> Vec<TokenStream> {
    let field_vars = field_vars(fields);

    field_vars
        .iter()
        .enumerate()
        .map(|(idx, field_var)| {
            let field = fields
                .iter()
                .nth(idx)
                .expect("field variable matches field");
            if is_phantom_data(&field.ty) {
                return quote!(#field_var);
            }

            let idx = syn::Index::from(idx);
            quote! { co3::borrow::Borrow::borrow(#field_var, &mut owner.#idx) }
        })
        .collect()
}

fn gen_record_construction(
    head: TokenStream,
    fields: &syn::Fields,
    values: &[impl quote::ToTokens],
) -> TokenStream {
    match fields {
        syn::Fields::Named(_) | syn::Fields::Unit => {
            let field_names: Vec<_> = fields.iter().filter_map(|f| f.ident.as_ref()).collect();

            quote! { #head { #(#field_names: #values),* } }
        }
        syn::Fields::Unnamed(_) => quote! { #head(#(#values),*) },
    }
}

fn gen_record_from_borrow(head: TokenStream, fields: &syn::Fields) -> TokenStream {
    let field_vars = field_vars(fields);

    let values = fields.iter().zip(&field_vars).map(|(field, field_var)| {
        if is_phantom_data(&field.ty) {
            quote!(#field_var)
        } else {
            quote!(co3::borrow::FromBorrow::from_borrow(#field_var))
        }
    });

    gen_record_construction(head, fields, &values.collect::<Vec<_>>())
}

fn rewrite_view_fields(data: &mut syn::Data) {
    match data {
        syn::Data::Struct(data) => rewrite_view_field_tys(&mut data.fields),
        syn::Data::Enum(data) => {
            for variant in &mut data.variants {
                rewrite_view_field_tys(&mut variant.fields);
            }
        }
        syn::Data::Union(_) => unreachable!(),
    }
}

fn rewrite_view_attrs(
    input: &mut DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
) {
    match &mut input.data {
        syn::Data::Struct(data) => {
            rewrite_view_repr_c_attrs(&mut input.attrs, attrs.is_valid.as_ref(), &data.fields)
        }
        syn::Data::Enum(data) => {
            for (variant, attrs) in data.variants.iter_mut().zip(variant_attrs) {
                rewrite_view_repr_c_attrs(
                    &mut variant.attrs,
                    attrs.is_valid.as_ref(),
                    &variant.fields,
                );
            }
        }
        _ => {}
    }
}

fn rewrite_view_repr_c_attrs(
    attrs: &mut Vec<syn::Attribute>,
    is_valid: Option<&syn::ExprClosure>,
    fields: &syn::Fields,
) {
    attrs.retain(|a| !a.path().is_ident("repr_c"));
    if is_valid.is_none() {
        return;
    }

    if let Some(is_valid) = is_valid {
        let view_is_valid = gen_view_is_valid_attr(is_valid, fields);
        attrs.push(parse_quote! { #[repr_c(is_valid = #view_is_valid)] });
    }
}

fn gen_view_is_valid_attr(is_valid: &syn::ExprClosure, fields: &syn::Fields) -> TokenStream {
    let field_vars = field_vars(fields);
    let field_types = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let args = field_vars
        .iter()
        .map(|field| quote!(#field))
        .collect::<Vec<_>>();
    let call = super::gen_is_valid_call(is_valid, &field_types, &args);

    quote! {
        |#(#field_vars),*| {
            #call
        }
    }
}

fn rewrite_view_generics(input: &mut DeriveInput) {
    let field_tys = match &input.data {
        syn::Data::Struct(data) => data.fields.iter().map(|f| &f.ty).collect::<Vec<_>>(),
        syn::Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|v| v.fields.iter().map(|f| &f.ty))
            .collect::<Vec<_>>(),
        syn::Data::Union(_) => unreachable!(),
    };

    let borrow_bounds =
        gen_field_trait_bounds(&input.generics, &field_tys, quote! { co3::borrow::Borrow })
            .collect::<Vec<_>>();

    if !field_tys.is_empty() {
        input.generics.params.insert(0, parse_quote!('_dšč));
        let where_clause = input.generics.make_where_clause();
        where_clause.predicates.push(parse_quote!(Self: '_dšč));
    }

    let where_clause = input.generics.make_where_clause();
    where_clause.predicates.extend(borrow_bounds);
}

fn rewrite_view_field_tys(fields: &mut syn::Fields) {
    for field in fields {
        let ty = &field.ty;

        if is_phantom_data(ty) {
            continue;
        }

        field.ty = parse_quote! {
            <#ty as co3::borrow::Borrow>::Borrowed<'_dšč>
        };
    }
}

pub(super) fn fields_are_only_phantom_data(data: &syn::Data) -> bool {
    let fields = match data {
        syn::Data::Struct(data) => data.fields.iter().collect::<Vec<_>>(),
        syn::Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| &variant.fields)
            .collect(),
        syn::Data::Union(_) => return false,
    };

    !fields.is_empty() && fields.into_iter().all(|field| is_phantom_data(&field.ty))
}

pub fn gen_either_name(len: usize) -> Ident {
    format_ident!("Either{len}")
}

pub fn either_variant_name(idx: usize) -> Ident {
    format_ident!("V{idx}")
}

pub(super) fn gen_view_owner_name(view_name: &Ident) -> Ident {
    let view_name_str = view_name.to_string();
    let owned_name = view_name_str.strip_suffix("View").unwrap();
    Ident::new(owned_name, view_name.span())
}

pub(super) fn gen_view_ctype_name(view_name: &Ident) -> Ident {
    gen_const_view_name(&gen_ctype_name(&gen_view_owner_name(view_name)))
}

pub(super) fn gen_const_view_name(name: &Ident) -> Ident {
    Ident::new(&format!("{name}ConstView"), name.span())
}

fn gen_view_name(name: &Ident) -> Ident {
    format_ident!("{name}View")
}

pub fn gen_identity_borrow_impls(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let mut from_borrow_generics = generics.clone();
    from_borrow_generics.params.insert(0, parse_quote!('d));
    let (from_borrow_impl_generics, _, _) = from_borrow_generics.split_for_impl();

    quote! {
        unsafe impl #impl_generics co3::borrow::Borrow for #name #ty_generics
        where
            for<'_dummy> Self: Sized,
            #predicates
        {
            type Borrowed<'_išč>
                = Self
            where
                Self: '_išč;

            type Owner = ();

            #[inline(always)]
            fn borrow<'_išč>(self, (): &mut ()) -> Self::Borrowed<'_išč>
            where
                Self: '_išč,
            {
                self
            }
        }

        impl #from_borrow_impl_generics co3::borrow::FromBorrow<'d> for #name #ty_generics
        where
            for<'_dummy> Self: Sized,
            #predicates
        {
            fn from_borrow(source: Self) -> Self {
                source
            }
        }
    }
}

pub fn gen_borrow_cast_eq_bounds(fields: &[&syn::Type]) -> TokenStream {
    let bounds = fields.iter().filter_map(|borrowed_ty| {
        if is_phantom_data(borrowed_ty) {
            return None;
        }

        let syn::Type::Path(syn::TypePath {
            qself: Some(syn::QSelf { ty, .. }),
            ..
        }) = borrowed_ty
        else {
            unreachable!()
        };

        Some(quote! {
            #borrowed_ty: co3::ReprC<
                CType = <<#ty as co3::ReprC>::CType as co3::borrow::BorrowCast>::AsConst
            >
        })
    });

    quote! { #(#bounds,)* }
}

fn gen_field_trait_bounds<'a>(
    generics: &'a syn::Generics,
    fields: &'a [&syn::Type],
    bound: TokenStream,
) -> impl Iterator<Item = syn::WherePredicate> + use<'a> {
    fields.iter().map(move |ty| {
        let for_dummy = (!is_type_parametrized(ty, generics)).then_some(quote!(for<'_dummy>));

        parse_quote! { #for_dummy #ty: #bound }
    })
}
