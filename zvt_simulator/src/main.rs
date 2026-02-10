use anyhow::Result;
use env_logger::{Builder, Env};
use std::io::Write;
use std::net::Ipv4Addr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Mutex;
use zvt::io::PacketTransport;

mod authorization;
mod end_of_day;
mod nack;
mod registration;

use authorization::handle_authorization;
use end_of_day::handle_end_of_day;
use nack::send_nack;
use registration::handle_registration;


fn init_logger() {
    let env = Env::default().filter_or("ZVT_LOGGER_LEVEL", "info");

    Builder::from_env(env)
        .format(|buf, record| {
            writeln!(
                buf,
                "{}: {}",
                match record.level() {
                    log::Level::Error => 3,
                    log::Level::Warn => 4,
                    log::Level::Info => 6,
                    log::Level::Debug => 7,
                    log::Level::Trace => 7,
                },
                record.args()
            )
        })
        .init();
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logger();

    // Create TCP listener
    // Use Ipv4Addr::UNSPECIFIED to bind to all interfaces, which is more portable than "0.0.0.0"
    // Use Ipv4Addr::LOCALHOST if you only want to accept connections from the local machine only (127.0.0.1)
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 22000)).await?;
    log::info!("Listening on 0.0.0.0:22000");

    // Create a new Arc<Mutex<bool>> to track registration state
    let registration = Arc::new(Mutex::new(false));
    loop {
        let (socket, peer_addr) = listener.accept().await?;
        log::info!("Client connected: {}", peer_addr);

        // Wrap the socket in PacketTransport
        let transport = PacketTransport { source: socket };

        // Spawn a task to handle this client
        let is_registered = registration.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(transport, is_registered).await {
                log::error!("Error handling client: {}", e);
            }
        });
    }
}
async fn handle_client(
    mut transport: PacketTransport<TcpStream>,
    is_registered: Arc<Mutex<bool>>,
) -> Result<()> {
    // Read packets in a loop until the socket is closed remotely
    loop {
        match transport.read_packet::<zvt::commands::Command>().await {
            Ok(packet) => {
                log::info!("Received packet: {:?}", packet);
                let result = match packet {
                    zvt::commands::Command::Ack(_) | zvt::commands::Command::Nack(_) => Ok(()),
                    // zvt::commands::Command::Nack(nack) => send_nack(&mut transport, nack).await,
                    zvt::commands::Command::Registration(reg) => {
                        if handle_registration(&mut transport, reg).await.is_ok() {
                            *is_registered.lock().await = true;
                        }
                        Ok(())
                    }
                    zvt::commands::Command::Authorization(auth) => {
                        if *is_registered.lock().await {
                            handle_authorization(&mut transport, auth).await
                        } else {
                            send_nack(&mut transport, zvt::packets::Nack { code: 0x83 }).await // FunctionNotPossible
                        }
                    }
                    zvt::commands::Command::EndOfDay(eod) => {
                        if *is_registered.lock().await {
                            handle_end_of_day(&mut transport, eod).await
                        } else {
                            send_nack(&mut transport, zvt::packets::Nack { code: 0x83 }).await // FunctionNotPossible
                        }
                    }
                    command => {
                        log::warn!("Unhandled command: {}", command.to_string());
                        send_nack(&mut transport, zvt::packets::Nack { code: 0x83 }).await // FunctionNotPossible
                    }
                };

                if let Err(e) = result {
                    log::error!("Error handling command: {e}");
                }
            }
            Err(e) => {
                // Check if it's an EOF error (socket closed)
                if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                    if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                        log::info!("Client disconnected");
                        break;
                    }
                }
                log::error!("Error reading packet: {}", e);
                // Continue reading for other errors
            }
        }
    }

    Ok(())
}
