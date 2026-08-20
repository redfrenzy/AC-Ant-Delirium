extern crate proc_macro;

use proc_macro::TokenStream;

#[proc_macro_derive(MatrixIdentity)]
pub fn derive_matrix_identity(_input: TokenStream) -> TokenStream {
    "impl MatrixValue { pub fn matrix_identity() -> &'static str { \"derived\" } }"
        .parse()
        .expect("static derive output")
}
