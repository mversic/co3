use proc_macro2::TokenStream;
use quote::quote;
use syn::ItemTrait;

pub(crate) fn derive_repr_c_trait(
    attr: TokenStream,
    item: TokenStream,
) -> syn::Result<TokenStream> {
}
