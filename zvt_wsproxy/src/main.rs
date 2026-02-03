use anyhow::Result;
use clap;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fs::File;
use std::path::PathBuf;
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;
use tracing::{error, info, warn};
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::EnvFilter;
type PacketTransport = zvt::io::PacketTransport<TcpStream>;
type WsSink = futures::stream::SplitSink<WebSocketStream<TcpStream>, tokio_tungstenite::tungstenite::Message>;

/*
    e2p Forwarding Commands:
    ECR to PT commands from WebSocket to Payment Terminal:
    - Ack (includes Nack)
    - Registration
    - Authorization

    p2e Forwarding Commands:
    PT to ECR commands from Payment Terminal to WebSocket
    - Ack
    - Nack
    - StatusInformation
    - PrintLine
    - CompletionData


    Beispiel, Websocket Nachrichten:
    --------------------------------

    {"command": "Connect", "payload": {"ip": "127.0.0.1", "port": 22000}}

    {"command": "Registration", "payload": {"password": 123456, "config_byte": 222, "currency": 978}}

    {"command": "Authorization", "payload": {"amount": 2500}}

    {"command":"Ack","payload":{"code":0}}

    // Bei Nack
    {"command":"Ack","payload":{"code":1, "message":"See ErrorMessages in constant.rs"}}
    {"command":"Ack","payload":{"code":100}}
    {"command":"Ack","payload":{"code":120}}
    {"command":"Ack","payload":{"code":255}}
*/


#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ack {
    pub code: i64,
    pub message: Option<String>,
}

impl From<zvt::packets::Nack> for Ack {
    fn from(nack: zvt::packets::Nack) -> Self {
        use num_traits::FromPrimitive;
        // let message = zvt::constants::ErrorMessages::from_u8(nack.code).map(|err| err.to_string());
        let message = zvt::constants::ErrorMessages::from_u8(nack.code)
            .map(|err| err.to_string());
        Ack {
            code: nack.code as i64,
            message,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connect {
    pub ip: String,
    pub port: u16,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Registration {
    pub password: usize,
    pub config_byte: u8,
    pub currency: Option<usize>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Authorization {
    pub amount: usize,
    pub currency: Option<usize>,
    pub payment_type: Option<u8>,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndOfDay {
    pub password: usize,
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntermediateStatusInformation {
    pub code: i64,
    pub message: Option<String>,
}

impl From<zvt::packets::IntermediateStatusInformation> for IntermediateStatusInformation {
    fn from(status: zvt::packets::IntermediateStatusInformation) -> Self {
        // Use status.status to translete these codes to these messages (cf zvt documentation, page 138)
        let message = match status.status {
            0x03 => Some("Not accepted".into()),
            0x0A => Some("Insert Card".into()),
            0x0D => Some("Processing error".into()),
            0x0E => Some("Please wait...".into()),
            0x17 => Some("Please wait...".into()),
            0x4B => Some("Please remove card!".into()),
            0xA0 => Some("Processing".into()), // Ingenico specific
            0xD2 => Some("Connecting dial-up".into()),
            _ => None,
        };
        IntermediateStatusInformation {
            code: status.status as i64,
            message,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInformation {
    pub amount: Option<usize>,
    pub trace_number: Option<usize>,
    pub time: Option<usize>,
    pub date: Option<usize>,
    pub expiry_date: Option<usize>,
    pub card_sequence_number: Option<usize>,
    pub card_type: Option<u8>,
    pub card_number: Option<usize>,
    pub track_2_data: Option<String>,
    pub result_code: Option<u8>,
    pub terminal_id: Option<usize>,
    pub vu_number: Option<String>,
    pub aid_authorization_attribute: Option<String>,
    pub additional_text: Option<String>,
    pub receipt_no: Option<usize>,
    pub currency: Option<usize>,
    pub zvt_card_type: Option<u8>,
    pub card_name: Option<String>,
    pub zvt_card_type_id: Option<u8>,
}

impl From<zvt::packets::StatusInformation> for StatusInformation {
    fn from(status_info: zvt::packets::StatusInformation) -> Self {
        StatusInformation {
            amount: status_info.amount,
            trace_number: status_info.trace_number,
            time: status_info.time,
            date: status_info.date,
            expiry_date: status_info.expiry_date,
            card_sequence_number: status_info.card_sequence_number,
            card_type: status_info.card_type,
            card_number: status_info.card_number,
            track_2_data: status_info.track_2_data,
            result_code: status_info.result_code,
            terminal_id: status_info.terminal_id,
            vu_number: status_info.vu_number,
            aid_authorization_attribute: status_info.aid_authorization_attribute,
            additional_text: status_info.additional_text,
            receipt_no: status_info.receipt_no,
            currency: status_info.currency,
            zvt_card_type: status_info.zvt_card_type,
            card_name: status_info.card_name,
            zvt_card_type_id: status_info.zvt_card_type_id,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintLine {
    // pub attribute: u8,
    pub line: String,
}

impl From<zvt::packets::PrintLine> for PrintLine {
    fn from(print_line: zvt::packets::PrintLine) -> Self {
        PrintLine {
            // attribute: print_line.attribute,
            line: print_line.text,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrintTextBlock {
    // pub receipt_type: u8,
    pub lines: Vec<String>,
}

impl From<zvt::packets::PrintTextBlock> for PrintTextBlock {
    fn from(print_text_block: zvt::packets::PrintTextBlock) -> Self {
        // let receipt_type = print_text_block
        //     .tlv
        //     .as_ref()
        //     .and_then(|tlv| tlv.receipt_type)
        //     .unwrap_or_default();
        let lines = print_text_block
            .tlv
            .as_ref()
            .and_then(|tlv| tlv.lines.as_ref())
            .map(|text_lines| text_lines.lines.clone())
            .unwrap_or_default();
        PrintTextBlock {
            // receipt_type,
            lines,
        }
    }
}

#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionData {
    pub result_code: Option<u8>,
    pub status_byte: Option<u8>,
    pub terminal_id: Option<usize>,
    pub currency: Option<usize>,
}

impl From<zvt::packets::CompletionData> for CompletionData {
    fn from(completion_data: zvt::packets::CompletionData) -> Self {
        CompletionData {
            result_code: completion_data.result_code,
            status_byte: completion_data.status_byte,
            terminal_id: completion_data.terminal_id,
            currency: completion_data.currency,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", content = "payload")]
pub enum Command {
    Connect(Connect),
    Registration(Registration),
    Authorization(Authorization),
    EndOfDay(EndOfDay),
    IntermediateStatusInformation(IntermediateStatusInformation),
    StatusInformation(StatusInformation),
    PrintLine(PrintLine),
    PrintTextBlock(PrintTextBlock),
    CompletionData(CompletionData),
    Abort(Ack),
    Ack(Ack),
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Command::Connect(_) => write!(f, "Connect"),
            Command::Registration(_) => write!(f, "Registration"),
            Command::Authorization(_) => write!(f, "Authorization"),
            Command::EndOfDay(_) => write!(f, "EndOfDay"),
            Command::IntermediateStatusInformation(_) => write!(f, "IntermediateStatusInformation"),
            Command::StatusInformation(_) => write!(f, "StatusInformation"),
            Command::PrintLine(_) => write!(f, "PrintLine"),
            Command::PrintTextBlock(_) => write!(f, "PrintTextBlock"),
            Command::CompletionData(_) => write!(f, "CompletionData"),
            Command::Abort(ack) => if ack.code == 0 { write!(f, "Abort") } else { write!(f, "Abort({})", ack.code) },
            Command::Ack(ack) => if ack.code == 0 { write!(f, "Ack") } else { write!(f, "Nack({})", ack.code) },
        }
    }
}

pub const VERSION: &str = "0.1.0";
// const BUILD_DATE: &str = env!("VERGEN_BUILD_DATE");
// const TARGET_TRIPLE: &str = env!("VERGEN_CARGO_TARGET_TRIPLE");
const DESCRIPTION: &str = clap::crate_description!();

fn setup_logging(log_file: Option<&PathBuf>) -> Option<()> {
    if let Some(log_file) = log_file {
        let log_file = File::create(log_file).expect("Cannot open logfile.");
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr.and(log_file))
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
            .compact()
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(
                EnvFilter::builder()
                    .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
            .compact()
            .init();
    };
    Some(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Setup CLI arguments
    let cli = clap::Command::new("pdl-agent")
        .version(VERSION)
        .about(DESCRIPTION)
        .args(&[
            clap::Arg::new("log")
                .short('l')
                .long("log")
                .value_name("file")
                .value_parser(clap::value_parser!(PathBuf))
                .help("Log to file"),
            clap::Arg::new("port")
                .short('p')
                .long("port")
                .value_parser(clap::value_parser!(u16))
                .required(true)
                .help("Listen port of webserver"),
        ])
        .get_matches();

    // Setup logging to stdout and optional into given file.
    let log = cli.get_one::<PathBuf>("log");
    setup_logging(log);

    // Log version information and given command line arguments.
    info!(
        target: "app",
        version = VERSION,
        "zvt_wsproxy",
    );
    info!(target: "cli", "Args: {:?}", std::env::args().collect::<Vec<String>>());

    // Get API port from CLI
    let port = *cli.get_one::<u16>("port").expect("port is required");

    // Start WebSocket server
    start_websocket_server(port).await?;

    // Shutdown
    info!(target: "app", "Shutting down");
    Ok(())
}

async fn start_websocket_server(port: u16) -> Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!(target: "app", "WebSocket server listening on ws://{}", addr);

    loop {
        let (stream, peer_addr) = listener.accept().await?;
        info!(target: "app", "New connection from {}", peer_addr);

        tokio::spawn(async move {
            if let Err(e) = handle_websocket(stream).await {
                tracing::error!(target: "app", "WebSocket error: {}", e);
            }
        });
    }
}

async fn handle_websocket(stream: TcpStream) -> Result<()> {
    let ws_stream = tokio_tungstenite::accept_async(stream).await?;
    info!(target: "app", "WebSocket connection established");

    let mut tcp = Option::<PacketTransport>::None;
    let (mut ws_tx, mut ws_rx) = ws_stream.split();

    loop {
        tokio::select! {
            // Handle incoming WebSocket messages
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                        // info!(target: "app", "Received text message: {}", text);

                        // Try to parse as JSON and deserialize to Command
                        match serde_json::from_str::<Command>(&text) {
                            Ok(cmd) => {
                                // info!(target: "app", "Successfully deserialized command: {:?}", cmd);
                                if let Command::Connect(connect) = cmd {
                                    tcp = do_connect(connect, &mut ws_tx).await.ok().flatten();
                                } else {
                                    // Handle all other commands
                                    match tcp.as_mut() {
                                        Some(tcp) => {
                                            // TCP connection is established, handle the command
                                            match cmd {
                                                Command::Registration(registration) => {
                                                    _ = e2p_forward_registration(registration, tcp, &mut ws_tx).await
                                                }
                                                Command::Authorization(authorization) => {
                                                    _ = e2p_forward_authorization(authorization, tcp, &mut ws_tx).await;
                                                }
                                                Command::EndOfDay(end_of_day) => {
                                                    _ = e2p_forward_end_of_day(end_of_day, tcp, &mut ws_tx).await;
                                                }
                                                Command::Ack(ack) => {
                                                    _ = e2p_forward_ack(ack, tcp, &mut ws_tx).await;
                                                }
                                                _ => {
                                                    warn!(target: "app", "Unhandled command: {}", cmd.to_string());
                                                    _ = ws_send_ack(&mut ws_tx,
                                                        "No forwarder implemented",
                                                        &format!("Unhandled command: {}", cmd.to_string())
                                                    ).await;
                                                }
                                            }
                                        }
                                        None => {
                                            warn!(target: "app", "No connection. Dropping command: {}", cmd.to_string());
                                            // No TCP connection established
                                            _ = ws_send_ack(&mut ws_tx,
                                                "No connection to terminal established",
                                                &format!("Failed to send command: {}", cmd.to_string())
                                            ).await;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                error!(target: "app", "Failed to deserialize command: {}", e);
                                // No TCP connection established
                                _ = ws_send_ack(&mut ws_tx,
                                    &e.to_string(),
                                    "Failed to deserialize command"
                                ).await;
                            }
                        }
                    }
                    Some(Ok(tokio_tungstenite::tungstenite::Message::Close(_))) => {
                        info!(target: "app", "Client closed connection");
                        break;
                    }
                    Some(Err(e)) => {
                        error!(target: "app", "WebSocket error: {}", e);
                        break;
                    }
                    None => {
                        error!(target: "app", "WebSocket stream ended");
                        break;
                    }
                    _ => {}
                }
            }

            // Handle incoming TCP packets from terminal (when connected)
            tcp_packet = async {
                tcp.as_mut().unwrap().read_packet::<zvt::commands::Command>().await
            }, if tcp.is_some() => {
                match tcp_packet {
                    Ok(cmd) => {
                        info!(target: "app", "Received packet from terminal: {:?}", cmd);

                        // Match specific command types for p2e forwarding (PT to ECR)
                        match cmd {
                            zvt::commands::Command::Ack(_) => {
                                let ack = zvt::packets::Nack { code: 0 };
                                _ = p2e_forward_nack(ack, &mut ws_tx).await;
                            }
                            zvt::commands::Command::Nack(nack) => {
                                _ = p2e_forward_nack(nack, &mut ws_tx).await;
                            }
                            zvt::commands::Command::Abort(abort) => {
                                let nack = zvt::packets::Nack { code: abort.error };
                                _ = p2e_forward_abort(nack, &mut ws_tx).await;
                            }
                            zvt::commands::Command::IntermediateStatusInformation(status_info) => {
                                _ = p2e_forward_intermediate_status_information(status_info, &mut ws_tx).await;
                                // _ = e2p_forward_ack(Ack { code: 0, message: None }, tcp.as_mut().unwrap(), &mut ws_tx).await;
                            }
                            zvt::commands::Command::StatusInformation(status_info) => {
                                _ = p2e_forward_status_information(status_info, &mut ws_tx).await;
                                // _ = e2p_forward_ack(Ack { code: 0, message: None }, tcp.as_mut().unwrap(), &mut ws_tx).await;
                            }
                            zvt::commands::Command::PrintLine(print_line) => {
                                _ = p2e_forward_print_line(print_line, &mut ws_tx).await;
                                // _ = e2p_forward_ack(Ack { code: 0, message: None }, tcp.as_mut().unwrap(), &mut ws_tx).await;
                            }
                            zvt::commands::Command::PrintTextBlock(print_text_block) => {
                                _ = p2e_forward_print_text_block(print_text_block, &mut ws_tx).await;
                                // _ = e2p_forward_ack(Ack { code: 0, message: None }, tcp.as_mut().unwrap(), &mut ws_tx).await;
                            }
                            zvt::commands::Command::CompletionData(completion_data) => {
                                _ = p2e_forward_completion_data(completion_data, &mut ws_tx).await;
                                // _ = e2p_forward_ack(Ack { code: 0, message: None }, tcp.as_mut().unwrap(), &mut ws_tx).await;
                            }
                            _ => {
                                warn!(target: "app", "Received unhandled command from terminal: {}", cmd.to_string());
                            }
                        }
                    }
                    Err(e) => {
                        // Check if it's an EOF error (socket closed)
                        if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                            if io_err.kind() == std::io::ErrorKind::UnexpectedEof {
                                info!(target: "app", "TCP connection closed by terminal");
                                tcp = None;
                            } else {
                                error!(target: "app", "TCP read error: {}", e);
                            }
                        } else {
                            error!(target: "app", "TCP read error: {}", e);
                        }
                    }
                }
            }
        }
    }

    // Close TCP connection if still open (will be dropped automatically)
    if tcp.is_some() {
        info!(target: "app", "Closing TCP connection to terminal");
        drop(tcp);
    }

    info!(target: "app", "WebSocket connection closed");
    Ok(())
}

/// Helper function to send an error response via WebSocket and return formatted error
async fn ws_send_ack(
    ws: &mut WsSink,
    error: &str,
    context: &str,
) -> String {
    let ack = Ack {
        code: -1, // Generic error code
        message: Some(error.to_string()),
    };
    // Send error response to WebSocket (ignore send errors)
    let _ = ws.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&Command::Ack(ack)).unwrap_or_default(),
    ))
        .await;
    format!("{}: {}", context, error)
}

// Beispiel Connect:
// websocat ws://127.0.0.1:8080
// {"command": "Connect", "payload": {"ip": "127.0.0.1", "port": 22000}}
async fn do_connect(
    connect: Connect,
    ws: &mut WsSink,
) -> Result<Option<PacketTransport>, String> {
    info!(target: "app", ip = connect.ip, port = connect.port, "Attempting to connect");

    // Try to connect to the specified TCP address
    match tokio::net::TcpStream::connect(format!("{}:{}", connect.ip, connect.port)).await {
        Ok(stream) => {
            info!(target: "app", ip = connect.ip, port = connect.port, "Successfully connected");
            let response = serde_json::to_string(&Command::Ack(Ack {
                code: 0,
                message: None,
            })).unwrap();
            ws.send(tokio_tungstenite::tungstenite::Message::Text(response))
                .await
                .map_err(|e| format!("WS send failed: {}", e))?;
            Ok(Some(PacketTransport { source: stream }))
        }
        Err(e) => {
            let message = e.to_string();
            warn!(target: "app", ip = connect.ip, port = connect.port, err = message, "Failed to connect");

            // Using negative OS error codes for network errors
            let code: i64 = match e.kind() {
                std::io::ErrorKind::ConnectionRefused => -111,  // ECONNREFUSED (errno 111)
                std::io::ErrorKind::TimedOut => -110,           // ETIMEDOUT (errno 110)
                std::io::ErrorKind::ConnectionReset => -104,    // ECONNRESET (errno 104)
                std::io::ErrorKind::NotFound => -2,             // ENOENT (errno 2)
                std::io::ErrorKind::PermissionDenied => -13,    // EACCES (errno 13)
                std::io::ErrorKind::AddrInUse => -98,           // EADDRINUSE (errno 98)
                std::io::ErrorKind::AddrNotAvailable => -99,    // EADDRNOTAVAIL (errno 99)
                _ => -1,                                         // Generic error
            };
            let response = serde_json::to_string(&Command::Ack(Ack {
                code,
                message: Some(message),
            })).unwrap();
            ws.send(tokio_tungstenite::tungstenite::Message::Text(response))
                .await
                .map_err(|e| format!("WS send failed: {}", e))?;
            Ok(None)
        }
    }
}

// Beispiel Registration:
// websocat ws://127.0.0.1:8080
// {"command": "Registration", "payload": {"password": 123456, "config_byte": 222, "currency": 978}}
async fn e2p_forward_registration(
    registration: Registration,
    tcp: &mut PacketTransport,
    ws: &mut WsSink,
) -> Result<(), String> {
    // Send registration packet to payment terminal
    if let Err(e) = tcp.write_packet(&zvt::packets::Registration {
        password: registration.password,
        config_byte: registration.config_byte,
        currency: registration.currency,
        tlv: None,
    })
        .await
    {
        return Err(ws_send_ack(ws, &e.to_string(), "Failed to send registration packet").await);
    }
    Ok(())
}

// Beispiel Authorization:
// websocat ws://127.0.0.1:8080
// {"command": "Authorization", "payload": {"amount": 2500, "currency": 978, "payment_type": 64}}
async fn e2p_forward_authorization(
    authorization: Authorization,
    tcp: &mut PacketTransport,
    ws: &mut WsSink,
) -> Result<(), String> {
    // Send authorization packet to payment terminal
    if let Err(e) = tcp.write_packet(&zvt::packets::Authorization {
        amount: Some(authorization.amount),
        currency: authorization.currency,
        payment_type: authorization.payment_type,
        ..Default::default()
    })
        .await
    {
        return Err(ws_send_ack(ws, &e.to_string(), "Failed to send authorization packet").await);
    }
    Ok(())
}

// Beispiel End-of-Day:
// websocat ws://127.0.0.1:8080
// {"command": "EndOfDay", "payload": {"password": 123456}}
async fn e2p_forward_end_of_day(
    end_of_day: EndOfDay,
    tcp: &mut PacketTransport,
    ws: &mut WsSink,
) -> Result<(), String> {
    // Send end-of-day packet to payment terminal
    if let Err(e) = tcp.write_packet(&zvt::packets::EndOfDay {
        password: end_of_day.password
    })
        .await
    {
        return Err(ws_send_ack(ws, &e.to_string(), "Failed to send end-of-day packet").await);
    }
    Ok(())
}

// Beispiel Ack:
// websocat ws://127.0.0.1:8080
// {"command": "Ack", "payload": {"code": 0, "message": null}}
async fn e2p_forward_ack(
    ack: Ack,
    tcp: &mut PacketTransport,
    ws: &mut WsSink,
) -> Result<(), String> {
    if ack.code == 0 {
        // Convert Ack to ZVT Nack packet and send to terminal
        let ack = zvt::packets::Ack {};
        if let Err(e) = tcp.write_packet(&ack).await {
            return Err(ws_send_ack(ws, &e.to_string(), "Failed to send Ack packet").await);
        }
    } else {
        // Convert Ack to ZVT Nack packet and send to terminal
        let nack = zvt::packets::Nack { code: ack.code as u8 };
        if let Err(e) = tcp.write_packet(&nack).await {
            return Err(ws_send_ack(ws, &e.to_string(), "Failed to send Ack packet").await);
        }
    }
    Ok(())
}

// p2e Forwarding Functions (PT to ECR - from terminal to WebSocket)

async fn p2e_forward_nack(
    nack: zvt::packets::Nack,
    ws: &mut WsSink,
) -> Result<(), String> {
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&Command::Ack(nack.into())).unwrap_or_default(),
    ))
    .await
    .map_err(|e| format!("Failed to send Ack/Nack to WebSocket: {}", e))
}

async fn p2e_forward_abort(
    nack: zvt::packets::Nack,
    ws: &mut WsSink,
) -> Result<(), String> {
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&Command::Abort(nack.into())).unwrap_or_default(),
    ))
    .await
    .map_err(|e| format!("Failed to send Ack/Nack to WebSocket: {}", e))
}

// Beispiel: {"status": "info", "message": "StatusInformation received from terminal", "data": {...}}
async fn p2e_forward_intermediate_status_information(
    status_info: zvt::packets::IntermediateStatusInformation,
    ws: &mut WsSink,
) -> Result<(), String> {
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&Command::IntermediateStatusInformation(status_info.into())).unwrap_or_default(),
    ))
    .await
    .map_err(|e| format!("Failed to send IntermediateStatusInformation to WebSocket: {}", e))
}

// Beispiel: {"status": "info", "message": "StatusInformation received from terminal", "data": {...}}
async fn p2e_forward_status_information(
    status_info: zvt::packets::StatusInformation,
    ws: &mut WsSink,
) -> Result<(), String> {
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&Command::StatusInformation(status_info.into())).unwrap_or_default(),
    ))
    .await
    .map_err(|e| format!("Failed to send StatusInformation to WebSocket: {}", e))
}

// Beispiel: {"status": "info", "message": "PrintLine received from terminal", "data": {...}}
async fn p2e_forward_print_line(
    print_line: zvt::packets::PrintLine,
    ws: &mut WsSink,
) -> Result<(), String> {
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&Command::PrintLine(print_line.into())).unwrap_or_default(),
    ))
    .await
    .map_err(|e| format!("Failed to send PrintLine to WebSocket: {}", e))
}

// Beispiel: {"status": "info", "message": "PrintLine received from terminal", "data": {...}}
async fn p2e_forward_print_text_block(
    print_text_block: zvt::packets::PrintTextBlock,
    ws: &mut WsSink,
) -> Result<(), String> {
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&Command::PrintTextBlock(print_text_block.into())).unwrap_or_default(),
    ))
    .await
    .map_err(|e| format!("Failed to send PrintTextBlock to WebSocket: {}", e))
}

// Beispiel: {"status": "success", "message": "CompletionData received from terminal", "data": {...}}
async fn p2e_forward_completion_data(
    completion_data: zvt::packets::CompletionData,
    ws: &mut WsSink,
) -> Result<(), String> {
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        serde_json::to_string(&Command::CompletionData(completion_data.into())).unwrap_or_default(),
    ))
    .await
    .map_err(|e| format!("Failed to send CompletionData to WebSocket: {}", e))
}
