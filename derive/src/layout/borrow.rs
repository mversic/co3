use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Ident, parse_quote};

use crate::utils::speculative_marker;

use crate::layout::{
    ReprCAttrs, VariantReprCAttrs,
    attr::ReprKind,
    ctype::{classify_tail, gen_ctype_name, gen_fields_methods},
    generic_param_idents, is_parametrized, is_phantom_data,
    item::{field_vars, gen_fields_destructure},
};

pub(super) fn gen_item_view(
    input: &DeriveInput,
    attrs: &ReprCAttrs,
    variant_attrs: &[VariantReprCAttrs],
    _repr: Option<&ReprKind>,
) -> TokenStream {
    if attrs.is_view {
        return quote! {};
    }

    if matches!(&input.data, syn::Data::Struct(data) if data.fields.iter().any(|field| classify_tail(&field.ty, &input.generics).is_known_unsized()))
    {
        return quote! {};
    }

    if fields_are_only_phantom_data(&input.data) {
        return quote! {};
    }

    let mut view_def = input.clone();
    rewrite_view_attrs(&mut view_def, attrs, variant_attrs);
    view_def
        .attrs
        .retain(|attr| !attr.path().is_ident("rust_spec"));

    view_def.ident = gen_view_name(&input.ident);

    // The view is always referenced with explicit arguments. Retaining an
    // owner's default here would force it to satisfy view-only bounds at the
    // view's definition, even when the owner's default is unsized.
    for param in &mut view_def.generics.params {
        match param {
            syn::GenericParam::Type(param) => param.default = None,
            syn::GenericParam::Const(param) => param.default = None,
            syn::GenericParam::Lifetime(_) => {}
        }
    }

    rewrite_view_generics(&mut view_def);
    rewrite_view_fields(&mut view_def.data);
    let rust_spec_impl = gen_view_rust_spec(input, &view_def);
    let fields_methods = match &view_def.data {
        syn::Data::Struct(data) => gen_fields_methods(
            &view_def.ident,
            &view_def.generics,
            &data.fields,
            false,
            false,
        ),
        syn::Data::Enum(_) => quote! {},
        syn::Data::Union(_) => unreachable!(),
    };
    let owner_name = &input.ident;
    let (_, owner_ty_generics, _) = input.generics.split_for_impl();

    let known_unsized_tail = matches!(
        &input.data,
        syn::Data::Struct(data)
            if data.fields.iter().next_back().is_some_and(|field|
                classify_tail(&field.ty, &input.generics).is_known_unsized())
    );
    let view_name = &view_def.ident;
    let (_, view_ty_generics, _) = view_def.generics.split_for_impl();
    let inherited_niche = attrs
        .niche_value
        .as_ref()
        .filter(|_| !known_unsized_tail)
        .map(|_| {
            quote! {
                #[repr_c(NICHE = unsafe {
                    core::mem::transmute_copy::<_, <#view_name #view_ty_generics as co3::ReprC>::CType>(
                        &<#owner_name #owner_ty_generics as co3::niche::Niche>::NICHE
                    )
                })]
            }
        });

    quote! {
        #[derive(co3::ReprC)]
        #[repr_c(view)]
        #inherited_niche
        #[doc(hidden)]
        #view_def
        #rust_spec_impl
        #fields_methods
    }
}

fn gen_view_rust_spec(owner: &DeriveInput, view: &DeriveInput) -> TokenStream {
    let rust_spec = quote!(co3::rust_spec::RustSpec);
    let owner_name = &owner.ident;
    let owner_args = generic_param_idents(owner.generics.params.iter()).collect::<Vec<_>>();
    let owner_ty = if owner_args.is_empty() {
        quote!(#owner_name)
    } else {
        quote!(#owner_name<#(#owner_args),*>)
    };
    let view_name = &view.ident;
    let (impl_generics, ty_generics, where_clause) = view.generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let fields = match &view.data {
        syn::Data::Struct(data) => data
            .fields
            .iter()
            .map(|field| &field.ty)
            .collect::<Vec<_>>(),
        syn::Data::Enum(data) => data
            .variants
            .iter()
            .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
            .collect(),
        syn::Data::Union(_) => unreachable!(),
    };
    let (mutability, mutability_bounds) = if let [field] = fields.as_slice() {
        let speculative = speculative_marker(
            matches!(&owner.data, syn::Data::Struct(_)) && !is_parametrized(field, &owner.generics),
        );
        (
            quote!(<#field as co3::rust_spec::RustSpec>::Mutability),
            vec![quote!(#speculative #field: co3::rust_spec::RustSpec)],
        )
    } else {
        // Conservatively retain the owner's interior-storage classification for
        // aggregate borrowed views, including storage reachable through pointers.
        (quote!(<#owner_ty as #rust_spec>::Mutability), Vec::new())
    };

    quote! {
        unsafe impl #impl_generics #rust_spec for #view_name #ty_generics
        where
            #owner_ty: #rust_spec,
            #(#mutability_bounds,)*
            #predicates
        {
            type Layout = <#owner_ty as #rust_spec>::Layout;
            type Size = <#owner_ty as #rust_spec>::Size;
            type __Alignment = <#owner_ty as #rust_spec>::__Alignment;
            type Trap = <#owner_ty as #rust_spec>::Trap;
            type Niche = <#owner_ty as #rust_spec>::Niche;
            type Drop = <#owner_ty as #rust_spec>::Drop;
            type Mutability = #mutability;
            type Opacity = co3::rust_spec::NonOpaque;
            type __IndirectTrap = <#owner_ty as #rust_spec>::__IndirectTrap;
        }
    }
}

pub(super) fn gen_item_borrow_impls(input: &DeriveInput) -> TokenStream {
    if matches!(&input.data, syn::Data::Struct(data) if data.fields.iter().any(|field| classify_tail(&field.ty, &input.generics).is_known_unsized()))
    {
        return quote! {};
    }

    let fieldless_enum = matches!(&input.data, syn::Data::Enum(data)
        if data.variants.iter().all(|variant| matches!(variant.fields, syn::Fields::Unit)));
    let no_drop = gen_identity_borrow_impls_for_drop(
        &input.ident,
        &input.generics,
        quote!(co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::NoDrop>),
    );
    let structural_bound = quote!(co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::AutoDrop>);
    let structural = if fields_are_only_phantom_data(&input.data) || fieldless_enum {
        gen_identity_borrow_impls_for_drop(&input.ident, &input.generics, structural_bound)
    } else {
        match &input.data {
            syn::Data::Struct(data) => gen_struct_borrow_impls(
                &input.ident,
                &input.generics,
                &data.fields,
                structural_bound,
            ),
            syn::Data::Enum(data) => gen_enum_borrow_impls(
                &input.ident,
                &input.generics,
                &data.variants,
                structural_bound,
            ),
            syn::Data::Union(_) => unreachable!(),
        }
    };
    let whole = gen_with_drop_borrow_impls(&input.ident, &input.generics);
    quote! {
        // These private dispatch traits inherit the original traits' safety contracts.
        #[allow(clippy::missing_safety_doc)]
        const _: () = {
            use co3::borrow::Borrow;

            co3::disjoint_impls! {
                #[disjoint_impls(remote)]
                pub unsafe trait Borrow: Sized {
                    type Borrowed;

                    fn borrow(self) -> Self::Borrowed;
                    fn from_borrow(source: Self::Borrowed) -> Self;
                }

                #no_drop
                #structural
                #whole
            }
        };
    }
}

fn gen_with_drop_borrow_impls(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let drop_kind = fresh_drop_kind_ident(generics);
    let (_, ty_generics, _) = generics.split_for_impl();
    let mut borrow_generics = generics.clone();
    borrow_generics
        .params
        .push(parse_quote!(#drop_kind: co3::rust_spec::drop::DropKind));
    let drop_bound: syn::WherePredicate = parse_quote!(Self:
        co3::rust_spec::RustSpec<Drop = co3::rust_spec::drop::CustomDrop<#drop_kind>>);
    borrow_generics
        .make_where_clause()
        .predicates
        .push(drop_bound);
    let speculative = speculative_marker(generics.params.is_empty());
    let sized_bound: syn::WherePredicate = if generics.params.is_empty() {
        parse_quote!(#speculative Self: Sized)
    } else {
        parse_quote!(Self: Sized)
    };
    borrow_generics
        .make_where_clause()
        .predicates
        .push(sized_bound);
    let (impl_generics, _, where_clause) = borrow_generics.split_for_impl();

    quote! {
        unsafe impl #impl_generics Borrow for #name #ty_generics #where_clause {
            type Borrowed = co3::borrow::Borrowed<Self>;

            #[inline(always)]
            fn borrow(self) -> Self::Borrowed {
                co3::borrow::Borrowed::new(self)
            }

            #[inline(always)]
            fn from_borrow(source: Self::Borrowed) -> Self {
                co3::borrow::Borrowed::into_inner(source)
            }
        }

    }
}

fn fresh_drop_kind_ident(generics: &syn::Generics) -> Ident {
    (0..)
        .map(|index| format_ident!("__co3_drop_kind_{index}"))
        .find(|ident| !generics.type_params().any(|param| param.ident == *ident))
        .expect("a fresh generic parameter name must exist")
}

fn gen_structural_move_prologue() -> TokenStream {
    quote! {
        // This branch has only field drop glue. Move the fields out, then let their owners drop them.
        let __co3_self = core::mem::ManuallyDrop::new(self);
        let __co3_self_ptr = &__co3_self as *const core::mem::ManuallyDrop<Self> as *const Self;
    }
}

fn gen_struct_borrow_impls(
    name: &Ident,
    generics: &syn::Generics,
    fields: &syn::Fields,
    drop_bound: TokenStream,
) -> TokenStream {
    let fields_destructure = gen_fields_destructure(fields);
    let field_reads = field_vars(fields)
        .into_iter()
        .enumerate()
        .map(|(index, var)| {
            let member = fields
                .iter()
                .nth(index)
                .and_then(|field| field.ident.clone())
                .map(syn::Member::Named)
                .unwrap_or_else(|| syn::Member::Unnamed(index.into()));
            quote! {
                let #var = unsafe { core::ptr::read(&raw const (*__co3_self_ptr).#member) };
            }
        });

    let view_name = gen_view_name(name);
    let field_tys = fields.iter().map(|field| &field.ty).collect::<Vec<_>>();
    let borrowed_fields = gen_field_borrow_exprs(fields);
    let borrowed_view = gen_record_construction(quote! { #view_name }, fields, &borrowed_fields);
    let from_borrow_body = gen_record_from_borrow(quote!(Self), fields);
    let move_prologue = gen_structural_move_prologue();

    let (borrow_impl, from_borrow_impl) = (
        quote! {
            #move_prologue
            #(#field_reads)*
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
        borrow_impl,
        from_borrow_impl,
        drop_bound,
    )
}

fn gen_enum_borrow_impls(
    name: &Ident,
    generics: &syn::Generics,
    variants: &syn::punctuated::Punctuated<syn::Variant, syn::Token![,]>,
    drop_bound: TokenStream,
) -> TokenStream {
    let fields = variants
        .iter()
        .flat_map(|variant| variant.fields.iter().map(|field| &field.ty))
        .collect::<Vec<_>>();

    let view_name = gen_view_name(name);
    let move_prologue = gen_structural_move_prologue();
    let (variants_borrow, variants_from_borrow): (Vec<_>, Vec<_>) = variants
        .iter()
        .map(|variant| {
            let variant_name = &variant.ident;
            let field_vars = field_vars(&variant.fields);
            let destructure_fields = gen_fields_destructure(&variant.fields);
            let view_head = quote! { #view_name::#variant_name };
            let owned_head = quote! { Self::#variant_name };
            let borrowed_variant = gen_record_construction(view_head, &variant.fields, &field_vars);
            let from_borrow_body = gen_record_from_borrow(owned_head, &variant.fields);

            let field_reads = field_vars.iter().map(|field_var| {
                quote! { let #field_var = unsafe { core::ptr::read(#field_var) }; }
            });
            let borrow_stmts = field_vars
                .iter()
                .zip(&variant.fields)
                .filter(|(_, field)| !is_phantom_data(&field.ty))
                .map(|(field_var, _)| {
                    quote! {
                        let #field_var = co3::borrow::Borrow::borrow(#field_var);
                    }
                });

            (
                quote! {
                    Self::#variant_name #destructure_fields => {
                        #(#field_reads)*
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
        quote! {
            #move_prologue
            match unsafe { &*__co3_self_ptr } { #(#variants_borrow,)* }
        },
        quote! {
            match source { #(#variants_from_borrow,)* }
        },
    );

    gen_borrow_impls::<false>(
        name,
        generics,
        &fields,
        borrow_impl,
        from_borrow_impl,
        drop_bound,
    )
}

fn gen_borrow_impls<const ADD_SIZED: bool>(
    name: &Ident,
    generics: &syn::Generics,
    fields: &[&syn::Type],
    borrow_impl: TokenStream,
    from_borrow_impl: TokenStream,
    drop_trait_bound: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|w| &w.predicates);
    let view_ty_generics = generic_param_idents(&generics.params).collect::<Vec<_>>();
    let view_ty_generics = (!view_ty_generics.is_empty()).then(|| {
        quote! { <#(#view_ty_generics),*> }
    });
    let borrow_bounds = gen_field_trait_bounds(generics, fields, quote! { co3::borrow::Borrow });

    let view_name = gen_view_name(name);
    let speculative = speculative_marker(generics.params.is_empty());
    let sized_bound = ADD_SIZED.then(|| {
        let tail_bound = fields.last().map(|ty| {
            let speculative = speculative_marker(!is_parametrized(ty, generics));
            quote!(#speculative #ty: Sized,)
        });
        quote! { #speculative Self: Sized, #tail_bound }
    });
    let drop_bound = quote! { #speculative Self: #drop_trait_bound, };
    quote! {
        unsafe impl #impl_generics Borrow for #name #ty_generics
        where
            #drop_bound
            #(#borrow_bounds,)*
            #sized_bound
            #predicates
        {
            type Borrowed = #view_name #view_ty_generics;

            #[inline(always)]
            fn borrow(self) -> Self::Borrowed {
                #borrow_impl
            }

            #[inline(always)]
            fn from_borrow(source: Self::Borrowed) -> Self {
                #from_borrow_impl
            }
        }

    }
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

            let _ = idx;
            quote! { co3::borrow::Borrow::borrow(#field_var) }
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
            let field_values = field_names.iter().zip(values).map(|(name, value)| {
                let value = quote!(#value);
                if syn::parse2::<syn::Ident>(value.clone()).is_ok_and(|ident| ident == **name) {
                    quote!(#name)
                } else {
                    quote!(#name: #value)
                }
            });

            quote! { #head { #(#field_values),* } }
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
            quote!(co3::borrow::Borrow::from_borrow(#field_var))
        }
    });

    gen_record_construction(head, fields, &values.collect::<Vec<_>>())
}

fn rewrite_view_fields(data: &mut syn::Data) {
    match data {
        syn::Data::Struct(data) => {
            rewrite_view_field_tys(&mut data.fields);
            for field in data.fields.iter_mut() {
                field.vis = syn::Visibility::Inherited;
            }
        }
        syn::Data::Enum(data) => {
            for variant in &mut data.variants {
                rewrite_view_field_tys(&mut variant.fields);
                for field in variant.fields.iter_mut() {
                    field.vis = syn::Visibility::Inherited;
                }
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
        syn::Data::Struct(data) => rewrite_view_repr_c_attrs(
            &mut input.attrs,
            attrs.is_valid.as_ref(),
            false,
            &data.fields,
        ),
        syn::Data::Enum(data) => {
            for (variant, attrs) in data.variants.iter_mut().zip(variant_attrs) {
                rewrite_view_repr_c_attrs(
                    &mut variant.attrs,
                    attrs.is_valid.as_ref(),
                    attrs.allow_zero_tag,
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
    allow_zero_tag: bool,
    fields: &syn::Fields,
) {
    attrs.retain(|a| !a.path().is_ident("repr_c"));
    if allow_zero_tag {
        attrs.push(parse_quote! { #[repr_c(allow_zero_tag)] });
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

    let last_is_tail = matches!(&input.data, syn::Data::Struct(_));
    let borrow_bounds = field_tys
        .iter()
        .enumerate()
        .map(|(index, ty)| {
            let speculative = speculative_marker(
                last_is_tail
                    && index + 1 == field_tys.len()
                    && !is_parametrized(ty, &input.generics),
            );
            parse_quote!(#speculative #ty: co3::borrow::Borrow)
        })
        .collect::<Vec<syn::WherePredicate>>();

    let tail_bound: Option<syn::WherePredicate> = if let syn::Data::Struct(data) = &input.data {
        data.fields.iter().next_back().map(|field| {
            let ty = &field.ty;
            let speculative = speculative_marker(!is_parametrized(ty, &input.generics));
            parse_quote!(#speculative #ty: Sized)
        })
    } else {
        None
    };
    let where_clause = input.generics.make_where_clause();
    where_clause.predicates.extend(tail_bound);
    where_clause.predicates.extend(borrow_bounds);
}

fn rewrite_view_field_tys(fields: &mut syn::Fields) {
    for field in fields {
        let ty = &field.ty;

        if is_phantom_data(ty) {
            continue;
        }

        field.ty = parse_quote! {
            <#ty as co3::borrow::Borrow>::Borrowed
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
    gen_ctype_name(view_name)
}

fn gen_view_name(name: &Ident) -> Ident {
    format_ident!("{name}View")
}

pub fn gen_identity_borrow_impls(name: &Ident, generics: &syn::Generics) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let speculative = speculative_marker(generics.params.is_empty());

    quote! {
        unsafe impl #impl_generics co3::borrow::Borrow for #name #ty_generics
        where
            #speculative Self: Sized,
            #predicates
        {
            type Borrowed = Self;

            #[inline(always)]
            fn borrow(self) -> Self::Borrowed {
                self
            }

            fn from_borrow(source: Self) -> Self {
                source
            }
        }

    }
}

fn gen_identity_borrow_impls_for_drop(
    name: &Ident,
    generics: &syn::Generics,
    drop_trait_bound: TokenStream,
) -> TokenStream {
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    let predicates = where_clause.as_ref().map(|clause| &clause.predicates);
    let speculative = speculative_marker(generics.params.is_empty());
    let drop_bound = quote! { #speculative Self: #drop_trait_bound, };
    let sized_bound = quote! { #speculative Self: Sized, };
    quote! {
        unsafe impl #impl_generics Borrow for #name #ty_generics where
            #drop_bound
            #sized_bound
            #predicates
        {
            type Borrowed = Self;
            fn borrow(self) -> Self::Borrowed { self }

            fn from_borrow(source: Self::Borrowed) -> Self { source }
        }

    }
}

fn gen_field_trait_bounds<'a>(
    generics: &'a syn::Generics,
    fields: &'a [&syn::Type],
    bound: TokenStream,
) -> impl Iterator<Item = syn::WherePredicate> + use<'a> {
    fields.iter().map(move |ty| {
        let for_dummy = speculative_marker(!is_parametrized(ty, generics));

        parse_quote! { #for_dummy #ty: #bound }
    })
}
