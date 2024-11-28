use proc_macro::TokenStream;
use quote::{format_ident, quote};

pub(crate) fn impl_water_dial(ast: &syn::DeriveInput) -> TokenStream {
    let name = &ast.ident;
    let inst_name = &format_ident!("OBJ_{}", name.to_string().to_uppercase());

    quote! {
    // version-independent API
    #[export_name = "_version"]
    pub fn _version() -> i32 {
        ::water_wasm::v0::VERSION
    }

    // version-independent API
    #[export_name = "_role"]
    pub fn _role() -> i32 {
        ::water_wasm::common::Role::Dialer as i32
    }


    lazy_static! {
        static ref #inst_name: std::sync::Arc<
            std::sync::Mutex<
                std::boxed::Box< dyn ::water_wasm::v0::ReadWriteDial + Sized>>> = {
                    let m = std::sync::Arc::new(std::sync::Mutex::new(<#name>::new()));
                    m
                };
    }

    #[export_name = "_config"]
    pub fn _config(fd: i32) -> i32 {
        0
    }

    #[export_name = "_dial"]
    pub fn _dial(caller_conn_fd: i32) -> i32 {
        println!("Dialing...");
        let mut obj = #inst_name.lock().unwrap();
        obj.dial(caller_conn_fd)
    }


    // V0 API
    #[export_name = "_read"]
    pub fn _read() -> i32 {
        println!("Dialing...");
        let mut obj = #inst_name.lock().unwrap();
        // obj.Read(caller_conn_fd)
    }

    // V0 API
    #[export_name = "_write"]
    pub fn _write() -> i32 {
        0
    }

    // V0 API
    #[export_name = "_close"]
    pub fn _close() { }

        }
    .into()
}
