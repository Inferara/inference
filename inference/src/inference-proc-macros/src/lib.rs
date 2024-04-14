use proc_macro::TokenStream;

#[proc_macro]
pub fn inference(input: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro_attribute]
pub fn inference_spec(attr: TokenStream, item: TokenStream) -> TokenStream {
    TokenStream::new()
}

#[proc_macro_attribute]
pub fn inference_fun(attr: TokenStream, item: TokenStream) -> TokenStream {
    TokenStream::new()
}
