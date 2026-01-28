use anyhow::Result;
use tokio::net::TcpStream;
use zvt::io::PacketTransport;

pub async fn handle_end_of_day(
    transport: &mut PacketTransport<TcpStream>,
    _eod: zvt::packets::EndOfDay,
) -> Result<()> {
    log::info!("Handling EndOfDay");

    // Send ACK
    log::info!("Sending ACK");
    transport
        .write_packet::<zvt::packets::Ack>(&zvt::packets::Ack {})
        .await?;

    // Send IntermediateStatusInformation
    log::info!("Sending IntermediateStatusInformation (multiple times)");
    let stati = vec![
        0x0E, // Please wait...
        0xD2, // Connecting dial-up
        0x0E, // Please wait...
        0x0E, // Please wait...
        // 0x0E, // Please wait...
        // 0x0E, // Please wait...
        // 0x0E, // Please wait...
        // 0x0E, // Please wait...
        // 0x0E, // Please wait...
        // 0x0E, // Please wait...
        // 0x0E, // Please wait...
        // 0x0E, // Please wait...
        // 0x0E, // Please wait...
        // 0x0E, // Please wait...
        0x17, // Please wait...
    ];
    for status_code in stati {
        let status = zvt::packets::IntermediateStatusInformation {
            status: status_code,
            timeout: None,
        };
        transport
            .write_packet_with_ack::<zvt::packets::IntermediateStatusInformation>(&status)
            .await?;

        //delay by 1 second to simulate processing time
        tokio::time::sleep(std::time::Duration::from_millis(500)).await
    }

    // Sending Receipt Printout
    let receipt = zvt::packets::PrintTextBlock {
        tlv: Some(zvt::packets::tlv::PrintTextBlock {
            receipt_type: Some(3),
            lines: Some(zvt::packets::tlv::TextLines {
                lines: vec![
                    "".to_string(),
                    "Hallo Welt Laden".to_string(),
                    "Musterstr. 1".to_string(),
                    "123456 Aalen".to_string(),
                    "".to_string(),
                    "".to_string(),
                    "Datum:        08.01.2026".to_string(),
                    "Uhrzeit:    19:05:00 Uhr".to_string(),
                    "Trace-Nr.         002573".to_string(),
                    "".to_string(),
                    "Kassenschnitt".to_string(),
                    "".to_string(),
                    "Terminal-ID     68423481".to_string(),
                    "Beleg-Nr.    1071 - 1071".to_string(),
                    "".to_string(),
                    "Währung              EUR".to_string(),
                    "".to_string(),
                    "Karte    Anzahl      EUR".to_string(),
                    "".to_string(),
                    "girocard    1      0,17 ".to_string(),
                    "Masterc.    0      0,00 ".to_string(),
                    "Visa        0      0,00 ".to_string(),
                    "Maestro     0      0,00 ".to_string(),
                    "VPAY        0      0,00 ".to_string(),
                    "VISAEL      0      0,00 ".to_string(),
                    "------------------------".to_string(),
                    "Gesamt      1      0,17 ".to_string(),
                    "".to_string(),
                    "".to_string(),
                    "Cashback".to_string(),
                    "".to_string(),
                    "Karte    Anzahl      EUR".to_string(),
                    "".to_string(),
                    "Einreichung der".to_string(),
                    "digitalen Belege".to_string(),
                    "erfolgreich".to_string(),
                    "".to_string(),
                    "Anzahl der".to_string(),
                    "eingereichten Belege".to_string(),
                    "2/2".to_string(),
                    "".to_string(),
                    "Kassenschnitt".to_string(),
                    "erfolgreich".to_string(),
                ],
                eol: Some(240),
            }),
        }),
    };
    transport
        .write_packet_with_ack::<zvt::packets::PrintTextBlock>(&receipt)
        .await?;

    // Send IntermediateStatusInformation
    log::info!("Sending IntermediateStatusInformation");
    {
        let status = zvt::packets::IntermediateStatusInformation {
            status: 0x17, // Please wait...
            timeout: None,
        };
        transport
            .write_packet_with_ack::<zvt::packets::IntermediateStatusInformation>(&status)
            .await?;
    }

    // Send StatusInformation with end-of-day summary
    log::info!("Sending StatusInformation (EndOfDay Summary)");

    let status = zvt::packets::StatusInformation {
        amount: Some(17),
        trace_number: Some(2573),
        time: Some(190500),
        date: Some(108),
        result_code: Some(0),
        single_amounts: Some(zvt::packets::SingleAmounts {
            receipt_no_start: 1071,
            receipt_no_end: 1071,
            girocard: zvt::packets::NumAndTotal { num: 1, total: 17 },
            jcb: zvt::packets::NumAndTotal { num: 0, total: 0 },
            eurocard: zvt::packets::NumAndTotal { num: 0, total: 0 },
            amex: zvt::packets::NumAndTotal { num: 0, total: 0 },
            visa: zvt::packets::NumAndTotal { num: 0, total: 0 },
            diners: zvt::packets::NumAndTotal { num: 0, total: 0 },
            others: zvt::packets::NumAndTotal { num: 0, total: 0 },
        }),
        ..Default::default()
    };
    transport
        .write_packet_with_ack::<zvt::packets::StatusInformation>(&status)
        .await?;

    // Send CompletionData with end-of-day summary
    log::info!("Sending CompletionData (EndOfDay Summary)");
    let completion_data = zvt::packets::CompletionData {
        // status_byte: Some(0),
        // terminal_id: Some(52523535),
        // currency: Some(978),
        ..Default::default()
    };
    transport
        .write_packet::<zvt::packets::CompletionData>(&completion_data)
        .await?;

    Ok(())
}
