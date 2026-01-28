use anyhow::Result;
use tokio::net::TcpStream;
use zvt::io::PacketTransport;

pub async fn handle_registration(
    transport: &mut PacketTransport<TcpStream>,
    _reg: zvt::packets::Registration,
) -> Result<()> {
    // Send ACK
    log::info!("Sending ACK");
    transport.write_packet::<zvt::packets::Ack>(&zvt::packets::Ack {}).await?;

    // Send CompletionData
    log::info!("Sending CompletionData (that must be ACKed)");
    let completion_data = zvt::packets::CompletionData {
        // status_byte: Some(0),
        terminal_id: Some(68423481),
        currency: Some(978),
        ..Default::default()
    };
    transport
        .write_packet::<zvt::packets::CompletionData>(&completion_data)
        .await?;

    Ok(())
}
