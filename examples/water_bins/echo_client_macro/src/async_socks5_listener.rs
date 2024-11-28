use anyhow::Result;
use bincode::{self};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use tracing::info;

use std::net::{SocketAddr, ToSocketAddrs};
use std::{os::fd::FromRawFd, vec};

use crate::{StreamConfigV1, DIALER};
use water_wasm::{ConnStream, Dialer};

// ----------------------- Listener methods -----------------------
#[export_name = "_water_listen_v1"]
fn listen() {
    wrapper().unwrap();
}

fn _listener_creation() -> Result<i32, std::io::Error> {
    let global_conn = match DIALER.lock() {
        Ok(conf) => conf,
        Err(e) => {
            eprintln!("[WASM] > ERROR: {}", e);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "failed to lock config",
            ));
        }
    };

    // FIXME: hardcoded the filename for now, make it a config later
    let stream = StreamConfigV1::init(
        global_conn.config.local_address.clone(),
        global_conn.config.local_port,
        "LISTEN".to_string(),
    );

    let encoded: Vec<u8> = bincode::serialize(&stream).expect("Failed to serialize");

    let address = encoded.as_ptr() as u32;
    let size = encoded.len() as u32;

    let fd = unsafe { water_wasm::net::c::create_listen(address, size) };

    if fd < 0 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "failed to create listener",
        ));
    }

    info!(
        "[WASM] ready to start listening at {}:{}",
        global_conn.config.local_address, global_conn.config.local_port
    );

    Ok(fd)
}

#[tokio::main(flavor = "current_thread")]
async fn wrapper() -> std::io::Result<()> {
    let fd = _listener_creation().unwrap();

    // Set up pre-established listening socket.
    let standard = unsafe { std::net::TcpListener::from_raw_fd(fd) };
    // standard.set_nonblocking(true).unwrap();
    let listener = TcpListener::from_std(standard)?;

    info!("[WASM] Starting to listen...");

    loop {
        // Accept new sockets in a loop.
        let socket = match listener.accept().await {
            Ok(s) => s.0,
            Err(e) => {
                eprintln!("[WASM] > ERROR: {}", e);
                continue;
            }
        };

        // Spawn a background task for each new connection.
        tokio::spawn(async move {
            eprintln!("[WASM] > CONNECTED");
            match handle_incoming(socket).await {
                Ok(()) => eprintln!("[WASM] > DISCONNECTED"),
                Err(e) => eprintln!("[WASM] > ERROR: {}", e),
            }
        });
    }
}

// SS handle incoming connections
async fn handle_incoming(mut stream: TcpStream) -> std::io::Result<()> {
    let mut buffer = [0; 512];

    // Read the SOCKS5 greeting
    let nbytes = stream
        .read(&mut buffer)
        .await
        .expect("Failed to read from stream");

    println!("Received {} bytes: {:?}", nbytes, buffer[..nbytes].to_vec());

    if nbytes < 2 || buffer[0] != 0x05 {
        eprintln!("Not a SOCKS5 request");
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Not a SOCKS5 request",
        ));
    }

    let nmethods = buffer[1] as usize;
    if nbytes < 2 + nmethods {
        eprintln!("Incomplete SOCKS5 greeting");
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Incomplete SOCKS5 greeting",
        ));
    }

    // For simplicity, always use "NO AUTHENTICATION REQUIRED"
    stream
        .write_all(&[0x05, 0x00])
        .await
        .expect("Failed to write to stream");

    // Read the actual request
    let nbytes = stream
        .read(&mut buffer)
        .await
        .expect("Failed to read from stream");

    println!("Received {} bytes: {:?}", nbytes, buffer[..nbytes].to_vec());

    if nbytes < 7 || buffer[0] != 0x05 || buffer[1] != 0x01 {
        println!("Invalid SOCKS5 request");
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid SOCKS5 request",
        ));
    }

    // Extract address and port
    let addr: SocketAddr = match buffer[3] {
        0x01 => {
            // IPv4
            if nbytes < 10 {
                eprintln!("Incomplete request for IPv4 address");
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Incomplete request for IPv4 address",
                ));
            }
            let addr = std::net::Ipv4Addr::new(buffer[4], buffer[5], buffer[6], buffer[7]);
            let port = u16::from_be_bytes([buffer[8], buffer[9]]);
            SocketAddr::from((addr, port))
        }
        0x03 => {
            // Domain name
            let domain_length = buffer[4] as usize;
            if nbytes < domain_length + 5 {
                eprintln!("Incomplete request for domain name");
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "Incomplete request for domain name",
                ));
            }
            let domain =
                std::str::from_utf8(&buffer[5..5 + domain_length]).expect("Invalid domain string");

            println!("Domain: {}", domain);

            let port =
                u16::from_be_bytes([buffer[5 + domain_length], buffer[5 + domain_length + 1]]);

            println!("Port: {}", port);

            let domain_with_port = format!("{}:443", domain); // Assuming HTTPS

            // domain.to_socket_addrs().unwrap().next().unwrap()
            match domain_with_port.to_socket_addrs() {
                Ok(mut addrs) => match addrs.next() {
                    Some(addr) => addr,
                    None => {
                        eprintln!("Domain resolved, but no addresses found for {}", domain);
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!("Domain resolved, but no addresses found for {}", domain),
                        ));
                    }
                },
                Err(e) => {
                    eprintln!("Failed to resolve domain {}: {}", domain, e);
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Failed to resolve domain {}: {}", domain, e),
                    ));
                }
            }
        }
        _ => {
            eprintln!("Address type not supported");
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Address type not supported",
            ));
        }
    };

    // NOTE: create a new Dialer to dial any target address as it wants to
    // Add more features later -- connect to target thru rules (direct / server)

    // Connect to target address
    let mut tcp_dialer = Dialer::new();
    tcp_dialer.config.remote_address = addr.ip().to_string();
    tcp_dialer.config.remote_port = addr.port();

    tcp_dialer.dial().expect("Failed to dial");

    let target_stream = match tcp_dialer.file_conn.outbound_conn.file.unwrap() {
        ConnStream::TcpStream(s) => s,
        _ => {
            eprintln!("Failed to get outbound tcp stream");
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Failed to get outbound tcp stream",
            ));
        }
    };

    target_stream
        .set_nonblocking(true)
        .expect("Failed to set non-blocking");

    let target_stream =
        TcpStream::from_std(target_stream).expect("Failed to convert to tokio stream");

    // Construct the response based on the target address
    let response = match addr {
        SocketAddr::V4(a) => {
            let mut r = vec![0x05, 0x00, 0x00, 0x01];
            r.extend_from_slice(&a.ip().octets());
            r.extend_from_slice(&a.port().to_be_bytes());
            r
        }
        SocketAddr::V6(a) => {
            let mut r = vec![0x05, 0x00, 0x00, 0x04];
            r.extend_from_slice(&a.ip().octets());
            r.extend_from_slice(&a.port().to_be_bytes());
            r
        }
    };

    stream
        .write_all(&response)
        .await
        .expect("Failed to write to stream");

    let (mut client_read, mut client_write) = tokio::io::split(stream);
    let (mut target_read, mut target_write) = tokio::io::split(target_stream);

    let client_to_target = async move {
        let mut buffer = vec![0; 4096];
        loop {
            match client_read.read(&mut buffer).await {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    if (target_write.write_all(&buffer[0..n]).await).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    };

    let target_to_client = async move {
        let mut buffer = vec![0; 4096];
        loop {
            match target_read.read(&mut buffer).await {
                Ok(0) => {
                    break;
                }
                Ok(n) => {
                    if (client_write.write_all(&buffer[0..n]).await).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    };

    // Run both handlers concurrently
    tokio::join!(client_to_target, target_to_client);

    Ok(())
}
