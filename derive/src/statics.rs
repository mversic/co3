use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::{
    Co3Static, co3_path,
    codec::{Carrier, Conversion},
    symbol_name_value,
    utils::cfg_attrs,
};

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
                #co3::impls!((#ty): #co3::rust_spec::RustSpec<
                    Layout = #co3::rust_spec::Specified,
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

pub(crate) fn gen_export_static(item: Co3Static) -> TokenStream {
    let co3 = co3_path();
    let Co3Static {
        attrs,
        mutability,
        ident,
        ty,
        ..
    } = item;
    let cfg_attrs = cfg_attrs(&attrs).collect::<Vec<_>>();
    let symbol = attrs
        .iter()
        .find_map(symbol_name_value)
        .expect("static symbol name is synthesized before code generation");
    let c_ty = quote!(<#ty as #co3::ReprC>::CType);
    let source_check = if matches!(mutability, syn::StaticMutability::None) {
        quote!(__co3_assert_type(&#ident);)
    } else {
        quote!(__co3_assert_mut_type(&raw mut #ident);)
    };
    let static_assert = gen_static_assert(&ty);

    quote! {
        #(#cfg_attrs)*
        #static_assert

        #(#cfg_attrs)*
        const _: () = {
            trait __Co3Same<T> {}
            impl<T> __Co3Same<T> for T {}

            fn __co3_assert_type<T: __Co3Same<#c_ty>>(_: &T) {}
            fn __co3_assert_mut_type<T: __Co3Same<#c_ty>>(_: *mut T) {}
            fn __co3_check_source() {
                #source_check
            }
        };

        #(#cfg_attrs)*
        #[cfg(debug_assertions)]
        const _: () = {
            unsafe extern "C" {
                #[link_name = #symbol]
                static __co3_symbol: #c_ty;
            }

            struct __Co3SymbolProbe(*const #c_ty);
            unsafe impl Sync for __Co3SymbolProbe {}

            #[used]
            static __CO3_SYMBOL_PROBE: __Co3SymbolProbe =
                __Co3SymbolProbe(&raw const __co3_symbol);
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
    language: &Conversion,
) -> TokenStream {
    let (wrapper_ident, definition, value) = wrapper_definition(vis, ident, ty, mutable);

    let co3 = co3_path();
    // A concrete lifetime lets rustc resolve the function pointer's DecodeMove
    // store before checking the EmptyStore bound.
    let fn_ptr = matches!(ty, syn::Type::FnPtr(_));
    let lt = if fn_ptr {
        syn::parse_quote!('static)
    } else {
        syn::parse_quote!('__co3_decode)
    };
    let binder = (!fn_ptr).then(|| quote!(for<#lt>));
    let decoder = language.bound(Carrier::Value, Some(&lt), language.empty_store());
    let decode_bounds = quote!(#binder #ty: #decoder, <#ty as #co3::ReprC>::CType: Copy,);
    let decode = language.decode(
        Carrier::Value,
        quote!(#ty),
        fn_ptr.then_some(&lt),
        quote!(source),
        None,
    );
    let stored_lt = syn::parse_quote!('__co3_static_d);
    let stored = language.bound(Carrier::Move, Some(&stored_lt), quote!());
    let soft_bound = language.bound(Carrier::Value, Some(&stored_lt), quote!());
    let state = language.store();
    let soft = language.decode(
        Carrier::Value,
        quote!(#ty),
        Some(&stored_lt),
        quote!(source),
        Some(quote!(store)),
    );
    let unsafe_fn = mutable.then(|| quote!(unsafe));
    let source = if mutable {
        quote!(unsafe { core::ptr::read(core::ptr::addr_of!(#raw_ident)) })
    } else {
        quote!(unsafe { #raw_ident })
    };
    let shared = (!mutable).then(||quote!(
        for<'_dummy> #ty: #co3::marker::AbiCompatible,
        for<'_dummy> <#ty as #co3::rust_spec::RustSpec>::Mutability: __Co3InteriorTrap<Trap = #co3::rust_spec::layout::Robust>,
    ));
    let read = quote! {
        #[inline]
        pub #unsafe_fn fn read(&self) -> Option<#ty>
        where #shared for<'_dummy> #ty: Copy, #decode_bounds {
            let source = #source;
            #decode
        }
        #[inline]
        pub #unsafe_fn fn soft_read<#stored_lt>(&self, store: &#stored_lt mut <#ty as #stored>::#state) -> Option<#ty>
        where #shared for<'_dummy> #ty: #soft_bound + Copy, <#ty as #co3::ReprC>::CType: Copy {
            let source = #source;
            #soft
        }
    };
    let direct_access = (!mutable).then(|| {
        let (get_bounds, validate) = match language {
            Conversion::Checked(lang) => (quote! {
                for<'_dummy> <#ty as #co3::rust_spec::RustSpec>::Mutability: __Co3InteriorTrap<Trap = #co3::rust_spec::layout::Robust>,
                for<'_dummy> #ty: #co3::convert::AbiValidate<#lang> + Sync,
            }, quote! {
                if !unsafe { <#ty as #co3::convert::AbiValidate<#lang>>::is_valid(&#raw_ident) } {
                    return None;
                }
            }),
            Conversion::Unchecked => (quote! {
                for<'_dummy> #ty: #co3::marker::AbiCompatible + Sync,
            }, quote! {}),
        };
        quote! {
            #[inline]
            pub fn get(&self) -> Option<&#ty>
            where
                #get_bounds
            {
                #validate
                Some(unsafe { self.get_unchecked() })
            }

            #[inline]
            pub unsafe fn get_unchecked(&self) -> &#ty
            where for<'_dummy> #ty: #co3::marker::AbiCompatible,
            {
                unsafe { &*(&#raw_ident as *const _ as *const #ty) }
            }
        }
    });

    let encoder = language.bound(Carrier::Value, None, language.empty_store());
    let encode = |v| language.encode(Carrier::Value, quote!(#ty), v, None, None);
    let set = encode(quote!(value));
    let default = encode(quote!(<#ty as core::default::Default>::default()));
    let take = language.decode(Carrier::Value, quote!(#ty), None, quote!(old), None);
    let mutable_methods = mutable.then(|| quote! {
        #[inline]
        pub unsafe fn set(&self, value: #ty)
        where for<'_dummy> #ty: #encoder,
        {
            let value = #set;
            unsafe { core::ptr::write(core::ptr::addr_of_mut!(#raw_ident), value); }
        }

        #[inline]
        pub unsafe fn take(&self) -> Option<#ty>
        where
            for<'_dummy> #ty: #encoder + core::default::Default,
            #decode_bounds
        {
            let replacement = #default;
            let old = unsafe { core::ptr::replace(core::ptr::addr_of_mut!(#raw_ident), replacement) };
            #take
        }
    });

    let interior_trap = (!mutable).then(|| {
        quote! {
            trait __Co3InteriorTrap {
                type Trap;
            }
            impl __Co3InteriorTrap for #co3::rust_spec::mutability::WithoutInterior {
                type Trap = #co3::rust_spec::layout::Robust;
            }
            impl<R> __Co3InteriorTrap for #co3::rust_spec::mutability::WithInterior<R> {
                type Trap = R;
            }
            impl<I: #co3::rust_spec::mutability::ExclusiveKind + __Co3InteriorTrap>
                __Co3InteriorTrap for #co3::rust_spec::mutability::Exclusive<I>
            {
                type Trap = I::Trap;
            }
        }
    });

    quote! {
        #definition
        const _: () = {
            #interior_trap
            // This bound is intentionally inaccessible outside the expansion.
            #[allow(private_bounds, reason = "generated accessors use internal validation bounds")]
            impl #wrapper_ident {
                #direct_access
                #read
                #mutable_methods
            }
        };
        #value
    }
}

pub(crate) fn gen_extern_static(
    abi: &syn::Abi,
    block_attrs: &[syn::Attribute],
    item: Co3Static,
    language: &Conversion,
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
    let wrapper = gen_import_wrapper(&vis, &ident, &ty, &raw_ident, mutable, language);

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
