// =================== Imports & Modules =====================
use std::{io::Read, os::fd::FromRawFd, sync::Mutex};

use lazy_static::lazy_static;
use tracing::{info, Level};

use water_wasm::*;

pub mod async_socks5_listener;

// create a mutable global variable stores a pointer to the config
lazy_static! {
    static ref DIALER: Mutex<Dialer> = Mutex::new(Dialer::new());
    // static ref CONN: Mutex<Connection> = Mutex::new(Connection::new());
}

#[cfg(target_family = "wasm")]
#[export_name = "_water_init"]
pub fn _init(debug: bool) {
    if debug {
        tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    }

    info!("[WASM] running in _init");
}

#[cfg(not(target_family = "wasm"))]
pub fn _init(debug: bool) {
    if debug {
        tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    }

    info!("[WASM] running in _init");
}

#[export_name = "_water_set_inbound"]
pub fn _water_bridging(fd: i32) {
    let mut global_dialer = match DIALER.lock() {
        Ok(dialer) => dialer,
        Err(e) => {
            eprintln!("[WASM] > ERROR: {}", e);
            return;
        }
    };

    global_dialer.file_conn.set_inbound(
        fd,
        ConnStream::File(unsafe { std::fs::File::from_raw_fd(fd) }),
    );
}

#[export_name = "_water_set_outbound"]
pub fn _water_bridging_out(fd: i32) {
    let mut global_dialer = match DIALER.lock() {
        Ok(dialer) => dialer,
        Err(e) => {
            eprintln!("[WASM] > ERROR: {}", e);
            return;
        }
    };

    global_dialer.file_conn.set_outbound(
        fd,
        ConnStream::TcpStream(unsafe { std::net::TcpStream::from_raw_fd(fd) }),
    );
}

#[export_name = "_water_config"]
pub fn _process_config(fd: i32) {
    info!("[WASM] running in _process_config");

    let mut config_file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut config = String::new();
    match config_file.read_to_string(&mut config) {
        Ok(_) => {
            let config: Config = match serde_json::from_str(&config) {
                Ok(config) => config,
                Err(e) => {
                    eprintln!("[WASM] > _process_config ERROR: {}", e);
                    return;
                }
            };

            let mut global_dialer = match DIALER.lock() {
                Ok(dialer) => dialer,
                Err(e) => {
                    eprintln!("[WASM] > ERROR: {}", e);
                    return;
                }
            };

            // global_dialer.file_conn.config = config.clone();
            global_dialer.config = config;
        }
        Err(e) => {
            eprintln!(
                "[WASM] > WASM _process_config failed reading path ERROR: {}",
                e
            );
        }
    };
}

#[export_name = "_water_write"]
pub fn _write(bytes_write: i64) -> i64 {
    let mut global_dialer = match DIALER.lock() {
        Ok(dialer) => dialer,
        Err(e) => {
            eprintln!("[WASM] > ERROR: {}", e);
            return -1;
        }
    };

    match global_dialer
        .file_conn
        ._write_2_outbound(&mut DefaultEncoder, bytes_write)
    {
        Ok(n) => n,
        Err(e) => {
            eprintln!("[WASM] > ERROR in _write: {}", e);
            -1
        }
    }
}

#[export_name = "_water_read"]
pub fn _read() -> i64 {
    match DIALER.lock() {
        Ok(mut global_dialer) => {
            match global_dialer
                .file_conn
                ._read_from_outbound(&mut DefaultDecoder)
            {
                Ok(n) => n,
                Err(e) => {
                    eprintln!("[WASM] > ERROR in _read: {}", e);
                    -1
                }
            }
        }
        Err(e) => {
            eprintln!("[WASM] > ERROR: {}", e);
            -1
        }
    }
}

#[export_name = "_water_dial"]
pub fn _dial() {
    match DIALER.lock() {
        Ok(mut global_dialer) => {
            if let Err(e) = global_dialer.dial() {
                eprintln!("[WASM] > ERROR in _dial: {}", e);
            }
        }
        Err(e) => {
            eprintln!("[WASM] > ERROR: {}", e);
        }
    }
}
