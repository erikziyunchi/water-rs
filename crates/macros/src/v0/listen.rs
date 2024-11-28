use proc_macro::TokenStream;
use quote::quote;

pub(crate) fn impl_water_listen(ast: &syn::DeriveInput) -> TokenStream {
    let _name = &ast.ident;
    quote! {}.into()
}
