use anyhow::Result;
use tokio::net::TcpStream;
use zvt::io::PacketTransport;

pub async fn handle_authorization(
    transport: &mut PacketTransport<TcpStream>,
    auth: zvt::packets::Authorization,
) -> Result<()> {
    // For testing purposes, we also want to simulate aborts
    // Odd amounts will trigger an abort
    let abort = (auth.amount.unwrap_or_default() & 1) != 0;
    let not_accepted = auth.payment_type.unwrap_or_default() != 64;

    // Send ACK
    log::info!("Sending ACK");
    transport
        .write_packet::<zvt::packets::Ack>(&zvt::packets::Ack {})
        .await?;

    // Send IntermediateStatusInformation
    log::info!("Sending IntermediateStatusInformation (multiple times)");
    let stati = if !abort {
        if !not_accepted {
            vec![
                0x0A, // Insert Card
                0x0A, // Insert Card
                0x4B, // Please remove card!
                0xD2, // Connecting dial-up
                0x0E, // Please wait...
                0x0E, // Please wait...
                0x0E, // Please wait...
            ]
        } else {
            vec![
                0x03, // Not accepted
            ]
        }
    } else {
        vec![
            0x0A, // Insert Card
            0x0A, // Insert Card
            0x0D, // Processing error
        ]
    };
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

    // Send abort and return early, to simulate abort situation
    if abort {
        log::info!("Sending Authorization Abort");
        let abort_packet = zvt::packets::Abort {
            error: 0x6c, // abort via timeout or abort-key
        };
        transport
            .write_packet_with_ack::<zvt::packets::Abort>(&abort_packet)
            .await?;
        return Ok(());
    }

    // Send abort and return early, to simulate "not accepted" situation
    if not_accepted {
        log::info!("Sending Authorization Abort");
        let abort_packet = zvt::packets::Abort {
            error: 0x83, // function not possible
        };
        transport
            .write_packet_with_ack::<zvt::packets::Abort>(&abort_packet)
            .await?;
        return Ok(());
    }

    // Send StatusInformation
    log::info!("Sending StatusInformation (that must be ACKed)");

    let status = zvt::packets::StatusInformation {
        amount: Some(17),
        trace_number: Some(2570),
        time: Some(190332),
        date: Some(108),
        expiry_date: Some(2812),
        card_sequence_number: Some(1),
        card_type: Some(96),
        card_number: Some(6805711901234567890),
        result_code: Some(0),
        terminal_id: Some(68423481),
        aid_authorization_attribute: Some("912440".into()),
        additional_text: Some("Zahlung erfolgt".into()),
        receipt_no: Some(1071),
        currency: Some(978),
        turnover_record_number: Some(1071),
        zvt_card_type: Some(5),
        card_name: Some("Girocard".into()),
        ..Default::default()
    };
    transport
        .write_packet_with_ack::<zvt::packets::StatusInformation>(&status)
        .await?;

    // Send IntermediateStatusInformation
    log::info!("Sending IntermediateStatusInformation");
    {
        let status = zvt::packets::IntermediateStatusInformation {
            status: 0xA0, // ??? Ingenico proprietary status code - "processing"
            timeout: None,
        };
        transport
            .write_packet_with_ack::<zvt::packets::IntermediateStatusInformation>(&status)
            .await?;
    }

    // Sending Receipt Printout
    log::info!("Sending PrintTextBlock (Receipt Printout)");
    let receipt = zvt::packets::PrintTextBlock {
        tlv: Some(zvt::packets::tlv::PrintTextBlock {
            receipt_type: Some(2),
            lines: Some(zvt::packets::tlv::TextLines {
                lines: vec![
                    "".to_string(),
                    "* *  Kundenbeleg  * *".to_string(),
                    "Hallo Welt Laden".to_string(),
                    "Musterstr. 1".to_string(),
                    "123456 Aalen".to_string(),
                    "".to_string(),
                    "".to_string(),
                    "Datum:        08.01.2026".to_string(),
                    "Uhrzeit:    19:03:32 Uhr".to_string(),
                    "Beleg-Nr.           1071".to_string(),
                    "Trace-Nr.         002570".to_string(),
                    "".to_string(),
                    "Kartenzahlung".to_string(),
                    "Contactless".to_string(),
                    "girocard".to_string(),
                    "".to_string(),
                    "Nr.".to_string(),
                    "###############7890 0001".to_string(),
                    "Genehmigungs-Nr.  912440".to_string(),
                    "Terminal-ID     68423481".to_string(),
                    "Pos-Info       00 075 00".to_string(),
                    "AS-Zeit 08.01. 19:03 Uhr".to_string(),
                    "".to_string(),
                    "Weitere Daten 0000008001".to_string(),
                    "//1F0302//".to_string(),
                    "".to_string(),
                    "Betrag EUR          0,17".to_string(),
                    "".to_string(),
                    "".to_string(),
                    "Zahlung erfolgt".to_string(),
                    "".to_string(),
                    "Bitte Beleg aufbewahren".to_string(),
                    "".to_string(),
                    "".to_string(),
                    "".to_string(),
                ],
                eol: Some(240),
            }),
        }),
    };
    transport
        .write_packet_with_ack::<zvt::packets::PrintTextBlock>(&receipt)
        .await?;

    // Send CompletionData
    log::info!("Sending CompletionData");
    let completion_data = zvt::packets::CompletionData {
        // status_byte: Some(0),
        ..Default::default()
    };
    transport
        .write_packet::<zvt::packets::CompletionData>(&completion_data)
        .await?;

    // Ok(Some(zvt::commands::PtToEcrCommand::CompletionData(completion_data)))
    Ok(())
}
