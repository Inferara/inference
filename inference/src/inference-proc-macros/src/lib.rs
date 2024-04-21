#![no_std]
#![warn(clippy::all, clippy::pedantic)]

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};

#[proc_macro]
pub fn inference(input: TokenStream) -> TokenStream {
    input
}

#[proc_macro_attribute]
pub fn inference_spec(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn inference_fun(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut input_function = parse_macro_input!(item as ItemFn);
    let dead_code_attr = syn::parse_quote!(#[allow(dead_code)]);
    input_function.attrs.push(dead_code_attr);
    let clippy_no_effect_attr = syn::parse_quote!(#[allow(clippy::no_effect)]);
    input_function.attrs.push(clippy_no_effect_attr);
    TokenStream::from(quote! {
        #input_function
    })
}
