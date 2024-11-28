use proc_macro2::{Span, TokenStream, TokenTree};
use quote::{quote, quote_spanned, ToTokens};
use syn::parse::{Parse, ParseStream, Parser};
use syn::{braced, Attribute, Ident, Path, Signature, Visibility};

// syn::AttributeArgs does not implement syn::Parse
type AttributeArgs = syn::punctuated::Punctuated<syn::Meta, syn::Token![,]>;

struct FinalConfig {
    crate_name: Option<Path>,
}

/// Config used in case of the attribute not being able to build a valid config
const DEFAULT_ERROR_CONFIG: FinalConfig = FinalConfig { crate_name: None };

struct Configuration {
    is_test: bool, // unused for now as testing with C interface doesn't make a lot of sense yet.
    crate_name: Option<Path>,
}

impl Configuration {
    fn new(is_test: bool) -> Self {
        Configuration {
            is_test,
            crate_name: None,
        }
    }

    fn set_crate_name(&mut self, name: syn::Lit, span: Span) -> Result<(), syn::Error> {
        if self.crate_name.is_some() {
            return Err(syn::Error::new(span, "`crate` set multiple times."));
        }
        let name_path = parse_path(name, span, "crate")?;
        self.crate_name = Some(name_path);
        Ok(())
    }

    // Currently unused
    fn macro_name(&self) -> &'static str {
        if self.is_test {
            "water_wasm::test"
        } else {
            "water_wasm::main"
        }
    }

    fn build(&self) -> Result<FinalConfig, syn::Error> {
        Ok(FinalConfig {
            crate_name: self.crate_name.clone(),
        })
    }
}

fn parse_path(lit: syn::Lit, span: Span, field: &str) -> Result<Path, syn::Error> {
    match lit {
        syn::Lit::Str(s) => {
            let err = syn::Error::new(
                span,
                format!(
                    "Failed to parse value of `{}` as path: \"{}\"",
                    field,
                    s.value()
                ),
            );
            s.parse::<syn::Path>().map_err(|_| err.clone())
        }
        _ => Err(syn::Error::new(
            span,
            format!("Failed to parse value of `{}` as path.", field),
        )),
    }
}

fn build_config(
    _input: &ItemFn,
    args: AttributeArgs,
    is_test: bool,
) -> Result<FinalConfig, syn::Error> {
    let mut config = Configuration::new(is_test);
    let _macro_name = config.macro_name();

    for arg in args {
        match arg {
            syn::Meta::NameValue(namevalue) => {
                let ident = namevalue
                    .path
                    .get_ident()
                    .ok_or_else(|| {
                        syn::Error::new_spanned(&namevalue, "Must have specified ident")
                    })?
                    .to_string()
                    .to_lowercase();
                let lit = match &namevalue.value {
                    syn::Expr::Lit(syn::ExprLit { lit, .. }) => lit,
                    expr => return Err(syn::Error::new_spanned(expr, "Must be a literal")),
                };
                match ident.as_str() {
                    "crate" => {
                        config.set_crate_name(lit.clone(), syn::spanned::Spanned::span(lit))?;
                    }
                    name => {
                        let msg = format!(
                            "Unknown attribute {} is specified; expected one of: `crate`",
                            name,
                        );
                        return Err(syn::Error::new_spanned(namevalue, msg));
                    }
                }
            }
            syn::Meta::Path(path) => {
                let name = path
                    .get_ident()
                    .ok_or_else(|| syn::Error::new_spanned(&path, "Must have specified ident"))?
                    .to_string()
                    .to_lowercase();
                let name = name.as_str();
                let msg = format!(
                    "Unknown attribute {} is specified; expected one of: `crate`",
                    name
                );
                // let msg = match name.as_str() {
                // // [Disabled] Options that are present in tokio, but not in water. Left as example.
                //
                // "threaded_scheduler" | "multi_thread" => {
                //     format!(
                //         "Set the runtime flavor with #[{}(flavor = \"multi_thread\")].",
                //         macro_name
                //     )
                // }
                // "basic_scheduler" | "current_thread" | "single_threaded" => {
                //     format!(
                //         "Set the runtime flavor with #[{}(flavor = \"current_thread\")].",
                //         macro_name
                //     )
                // }
                // "flavor" | "worker_threads" | "start_paused" => {
                //     format!("The `{}` attribute requires an argument.", name)
                // }
                // name => {
                //     format!(
                //         "Unknown attribute {} is specified; expected one of: `crate`",
                //         name
                //     )
                // }
                // };
                return Err(syn::Error::new_spanned(path, msg));
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "Unknown attribute inside the macro",
                ));
            }
        }
    }

    config.build()
}

fn token_stream_with_error(mut tokens: TokenStream, error: syn::Error) -> TokenStream {
    tokens.extend(error.into_compile_error());
    tokens
}

pub(crate) fn entrypoint(args: TokenStream, item: TokenStream) -> TokenStream {
    //Args unused for now, but could be used to add attributes to the macro

    // If any of the steps for this macro fail, we still want to expand to an item that is as close
    // to the expected output as possible. This helps out IDEs such that completions and other
    // related features keep working.
    let input: ItemFn = match syn::parse2(item.clone()) {
        Ok(it) => it,
        Err(e) => return token_stream_with_error(item, e),
    };

    let config = if input.sig.ident == "entry" && !input.sig.inputs.is_empty() {
        let msg = "the main function cannot accept arguments";
        Err(syn::Error::new_spanned(&input.sig.ident, msg))
    } else {
        AttributeArgs::parse_terminated
            .parse2(args)
            .and_then(|args| build_config(&input, args, false))
    };

    match config {
        Ok(config) => expand_entrypoint(input, false, config),
        Err(e) => token_stream_with_error(expand_entrypoint(input, false, DEFAULT_ERROR_CONFIG), e),
    }
}

fn expand_entrypoint(input: ItemFn, is_test: bool, config: FinalConfig) -> TokenStream {
    // If type mismatch occurs, the current rustc points to the last statement.
    let (last_stmt_start_span, last_stmt_end_span) = {
        let mut last_stmt = input.stmts.last().cloned().unwrap_or_default().into_iter();

        // `Span` on stable Rust has a limitation that only points to the first
        // token, not the whole tokens. We can work around this limitation by
        // using the first/last span of the tokens like
        // `syn::Error::new_spanned` does.
        let start = last_stmt.next().map_or_else(Span::call_site, |t| t.span());
        let end = last_stmt.last().map_or(start, |t| t.span());
        (start, end)
    };

    let crate_path = config
        .crate_name
        .map(ToTokens::into_token_stream)
        .unwrap_or_else(|| Ident::new("water_wasm::v0", last_stmt_start_span).into_token_stream());

    let rt = quote_spanned! {last_stmt_start_span=>
        #crate_path::runtime::Builder::new_current_thread()
    };

    let header = if is_test {
        quote! {
            // version-independent API
            #[::core::prelude::v1::test]
            #[export_name = "_init"]
            pub fn _init() {}
        }
    } else {
        quote! {
            #[export_name = "_init"]
            pub fn _init() {}
        }
    };

    let body_ident = quote! { body };
    let last_block = quote_spanned! {last_stmt_end_span=>
        #[allow(clippy::expect_used, clippy::diverging_sub_expression)]
        {
            return #rt
                .enable_all()
                .build()
                .expect("Failed building the Runtime")
                .block_on(#body_ident);
        }
    };

    let body = input.body();

    // For test functions pin the body to the stack and use `Pin<&mut dyn
    // Future>` to reduce the amount of `Runtime::block_on` (and related
    // functions) copies we generate during compilation due to the generic
    // parameter `F` (the future to block on). This could have an impact on
    // performance, but because it's only for testing it's unlikely to be very
    // large.
    //
    // We don't do this for the main function as it should only be used once so
    // there will be no benefit.
    let body = if is_test {
        let output_type = match &input.sig.output {
            // For functions with no return value syn doesn't print anything,
            // but that doesn't work as `Output` for our boxed `Future`, so
            // default to `()` (the same type as the function output).
            syn::ReturnType::Default => quote! { () },
            syn::ReturnType::Type(_, ret_type) => quote! { #ret_type },
        };
        quote! {
            let body = async #body;
            #crate_path::pin!(body);
            let body: ::std::pin::Pin<&mut dyn ::std::future::Future<Output = #output_type>> = body;
        }
    } else {
        quote! {
            let body = async #body;
        }
    };

    input.into_tokens(header, body, last_block)
}

struct ItemFn {
    outer_attrs: Vec<Attribute>,
    vis: Visibility,
    sig: Signature,
    brace_token: syn::token::Brace,
    inner_attrs: Vec<Attribute>,
    stmts: Vec<proc_macro2::TokenStream>,
}

impl ItemFn {
    /// Access all attributes of the function item.
    #[allow(dead_code)]
    fn attrs(&self) -> impl Iterator<Item = &Attribute> {
        self.outer_attrs.iter().chain(self.inner_attrs.iter())
    }

    /// Get the body of the function item in a manner so that it can be
    /// conveniently used with the `quote!` macro.
    fn body(&self) -> Body<'_> {
        Body {
            brace_token: self.brace_token,
            stmts: &self.stmts,
        }
    }

    /// Convert our local function item into a token stream.
    fn into_tokens(
        self,
        header: proc_macro2::TokenStream,
        body: proc_macro2::TokenStream,
        last_block: proc_macro2::TokenStream,
    ) -> TokenStream {
        let mut tokens = proc_macro2::TokenStream::new();
        header.to_tokens(&mut tokens);

        // Outer attributes are simply streamed as-is.
        for attr in self.outer_attrs {
            attr.to_tokens(&mut tokens);
        }

        // Inner attributes require extra care, since they're not supported on
        // blocks (which is what we're expanded into) we instead lift them
        // outside of the function. This matches the behavior of `syn`.
        for mut attr in self.inner_attrs {
            attr.style = syn::AttrStyle::Outer;
            attr.to_tokens(&mut tokens);
        }

        self.vis.to_tokens(&mut tokens);
        self.sig.to_tokens(&mut tokens);

        self.brace_token.surround(&mut tokens, |tokens| {
            body.to_tokens(tokens);
            last_block.to_tokens(tokens);
        });

        tokens
    }
}

impl Parse for ItemFn {
    #[inline]
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        // This parse implementation has been largely lifted from `syn`, with
        // the exception of:
        // * We don't have access to the plumbing necessary to parse inner
        //   attributes in-place.
        // * We do our own statements parsing to avoid recursively parsing
        //   entire statements and only look for the parts we're interested in.

        let outer_attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        let sig: Signature = input.parse()?;

        let content;
        let brace_token = braced!(content in input);
        let inner_attrs = Attribute::parse_inner(&content)?;

        let mut buf = proc_macro2::TokenStream::new();
        let mut stmts = Vec::new();

        while !content.is_empty() {
            if let Some(semi) = content.parse::<Option<syn::Token![;]>>()? {
                semi.to_tokens(&mut buf);
                stmts.push(buf);
                buf = proc_macro2::TokenStream::new();
                continue;
            }

            // Parse a single token tree and extend our current buffer with it.
            // This avoids parsing the entire content of the sub-tree.
            buf.extend([content.parse::<TokenTree>()?]);
        }

        if !buf.is_empty() {
            stmts.push(buf);
        }

        Ok(Self {
            outer_attrs,
            vis,
            sig,
            brace_token,
            inner_attrs,
            stmts,
        })
    }
}

struct Body<'a> {
    brace_token: syn::token::Brace,
    // Statements, with terminating `;`.
    stmts: &'a [TokenStream],
}

impl ToTokens for Body<'_> {
    fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
        self.brace_token.surround(tokens, |tokens| {
            for stmt in self.stmts {
                stmt.to_tokens(tokens);
            }
        })
    }
}
