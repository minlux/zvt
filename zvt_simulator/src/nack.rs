use anyhow::Result;
use tokio::net::TcpStream;
use zvt::io::PacketTransport;

pub async fn send_nack(
    transport: &mut PacketTransport<TcpStream>,
    nack: zvt::packets::Nack,
) -> Result<()> {
    transport.write_packet::<zvt::packets::Nack>(&nack).await?;
    Ok(())
}
