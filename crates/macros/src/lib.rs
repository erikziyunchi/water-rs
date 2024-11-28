#[allow(unused_extern_crates)]
extern crate proc_macro;
extern crate proc_macro2;

extern crate quote;
extern crate syn;

mod v0;
use v0::{entrypoint, impl_water_dial, impl_water_listen, impl_water_wrap};

use proc_macro::TokenStream;
use syn::{parse_macro_input, DeriveInput};

/// Marks async function to be executed by the selected runtime. This macro
/// helps set up a `Runtime` without requiring the user to use
/// [Runtime](../tokio/runtime/struct.Runtime.html) or
/// [Builder](../tokio/runtime/struct.Builder.html) directly.
///
/// Note: This macro is designed to be simplistic and targets applications that
/// do not require a complex setup. If the provided functionality is not
/// sufficient, you may be interested in using
/// [Builder](../tokio/runtime/struct.Builder.html), which provides a more
/// powerful interface.
///
/// Note: This macro can be used on any function and not just the `main`
/// function. Using it on a non-main function makes the function behave as if it
/// was synchronous by starting a new runtime each time it is called. If the
/// function is called often, it is preferable to create the runtime using the
/// runtime builder so the runtime can be reused across calls.
///
/// # Non-worker async function
///
/// Note that the async function marked with this macro does not run as a
/// worker. The expectation is that other tasks are spawned by the function here.
/// Awaiting on other futures from the function provided here will not
/// perform as fast as those spawned as workers.
///
/// ```
/// #[water_macros_v0::entry]
/// # fn entry() {}
/// ```
/// ## Usage
///
/// ### Using the multi-thread runtime
///
/// ```rust
/// #[water_macros_v0::entry]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[water_macros_v0::entry]`
///
/// ```rust
/// fn entry() {
///     tokio::runtime::Builder::new_multi_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### Rename package
///
/// ```rust
/// use water_macros_v0 as wtr0;
///
/// #[water_macros_v0::entry(crate = "wtr0")]
/// fn entry() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[tokio::main]`
///
/// ```rust
/// use water_macros_v0 as wtr0;
///
/// fn entry() {
///     tokio1::runtime::Builder::new_multi_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
#[proc_macro_attribute]
// #[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn entry(args: TokenStream, item: TokenStream) -> TokenStream {
    entrypoint(args.into(), item.into()).into()
}

// // Disabled for now since Testing with C interface doesn't make a lot if sense yet.
// #[cfg(test)]
// #[proc_macro_attribute]
// pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
//     entry::test(args.into(), item.into()).into()
// }

#[proc_macro_derive(WaterDialer)]
pub fn water_dial_macro(input: TokenStream) -> TokenStream {
    // Parse the string representation
    let ast = parse_macro_input!(input as DeriveInput);

    // Build the impl
    impl_water_dial(&ast)
}

#[proc_macro_derive(WaterListener)]
pub fn water_listen_macro(input: TokenStream) -> TokenStream {
    // Parse the string representation
    let ast = parse_macro_input!(input as DeriveInput);

    // Build the impl
    impl_water_listen(&ast)
}

#[proc_macro_derive(WaterWrapper)]
pub fn water_wrap_macro(input: TokenStream) -> TokenStream {
    // Parse the string representation
    let ast = parse_macro_input!(input as DeriveInput);

    // Build the impl
    impl_water_wrap(&ast)
}
