use std::{ collections::HashMap, time::Duration, vec };
use cobs::{ decode };
use tracing::*;
use tracing_subscriber::{
    fmt::{ self, format::FmtSpan },
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};
use tracing_appender::rolling;

use pingpong_core::{ arduino::{ Giga, BAUD, Action, Command, StateMessage } };

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let kwargs: HashMap<String, String> = args
        .iter()
        .skip(2)
        .filter_map(|arg| {
            let mut parts = arg.splitn(2, '=');
            if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                Some((key.to_string().to_lowercase(), value.to_string()))
            } else {
                None
            }
        })
        .collect();
    let port_name = if args.len() > 1 {
        &args[1]
    } else {
        "COM5" // 默認端口
    };

    let debug_mode = kwargs
        .get_key_value("debug")
        .map_or(false, |(_, v)| (v == "true" || v == "1"));
    let show_byte = kwargs
        .get_key_value("show_byte")
        .map_or(false, |(_, v)| (v == "true" || v == "1"));

    let sensor_monitor = kwargs.get("sensor_monitor").map_or(false, |v| (v == "true" || v == "1"));

    let timeout = kwargs
        .get("timeout")
        .and_then(|v| v.parse::<u64>().ok())
        .map_or(Duration::from_secs(1), Duration::from_secs);

    // let proj_exe = std::env::current_exe().unwrap();
    // let proj_root_dir = proj_exe.parent().unwrap();
    // let log_dir = proj_root_dir.join("logs");
    let dir_name = "logs";
    let file_name = "cbor_test.log";
    // 1. 準備檔案 appender（logs/YYYY-MM-DD.log）
    let file_app = rolling::daily(dir_name, file_name);
    let (file_writer, guard) = tracing_appender::non_blocking(file_app);

    // 2. 建 stdout layer
    let stdout_layer = fmt
        ::layer()
        .with_writer(std::io::stdout) // 終端輸出
        .without_time() // 不印時間
        .with_target(false) // 不印 module 名
        .with_file(false) // 顯示檔案名稱
        .with_line_number(false) // 顯示行號
        .with_thread_ids(true) // 顯示線程 ID
        // .with_thread_names(true) // 顯示線程名稱
        .with_span_events(FmtSpan::NONE) // 顯示 span 事件
        .with_ansi(true); // 顯示顏色

    // 3. 建 file layer
    let file_layer = fmt
        ::layer()
        .with_writer(file_writer) // 背景 thread 寫檔
        .with_target(false) // 顯示模組路徑（target）
        .with_file(true) // 顯示檔案名稱
        .with_line_number(true) // 顯示行號
        .with_thread_ids(true) // 顯示線程 ID
        // .with_thread_names(true) // 顯示線程名稱
        .with_span_events(FmtSpan::NONE) // 顯示 span 事件
        .with_ansi(false); // 檔案不要色碼

    // 4. 裝上去 & init
    tracing_subscriber
        ::registry()
        .with(stdout_layer)
        .with(file_layer)
        .with(EnvFilter::new(if debug_mode { "debug" } else { "info" })) // 或 EnvFilter::from_default_env()
        .init();

    // 5. **保留 guard**（否則 app 結束前可能 flush 不到）
    let _guard = guard;

    info!("CBOR Test 開始 ================================================================");
    info!("使用串口: {}", port_name);
    info!("DEBUG 模式: {}", debug_mode);
    info!("Show Byte: {}", show_byte);
    info!("超時設定: {:?}", timeout);
    info!("Sensor Monitor Mode: {}", sensor_monitor);

    // payload
    let payload = StateMessage { status: 0 };
    info!("{:30} {}, size: {}", "PayLoad:", payload, std::mem::size_of_val(&payload));

    // 將 payload 序列化為 CBOR 格式
    let payload_cbor = serde_cbor::to_vec(&payload)?;
    info!("{:30} {:02X?}, size: {}", "PayLoad CBOR:", payload_cbor, payload_cbor.len());

    // 建立要傳送的 frame
    let (frame, cobs_size, crc) = Giga::build_cobs_frame(
        Action::READ,
        Command::Sensor,
        &payload_cbor
    );
    let msg = format!(
        "{:30} {:02X?}, len: {}, {:02X?}, CRC: {:02X?}",
        "Send CBOR without COBS Frame:",
        frame,
        cobs_size,
        (cobs_size as u16).to_le_bytes(),
        crc.to_le_bytes()
    );
    info!("{}", msg);

    // 建立 COBS 編碼的 frame
    let (cobs_frame, _cobs_size, crc) = Giga::build_cobs_frame(
        Action::SEND,
        Command::MOTOR,
        &payload_cbor
    );
    let msg = format!(
        "{:30} {:02X?}, size: {}, {:02X?}, CRC: {:02X?}",
        "CBOR with COBS Frame:",
        cobs_frame,
        _cobs_size,
        (_cobs_size as u16).to_le_bytes(),
        crc.to_le_bytes()
    );
    info!("{}", msg);

    // 將 COBS 編碼的 frame 包裝成完整的傳送 frame
    // 這裡假設 START_BYTE 為 0x00，實際應根據協議定義
    let send_frame = vec![0x00].into_iter().chain(cobs_frame.clone()).collect::<Vec<u8>>();
    let send_frame_size = send_frame.len();
    let msg = format!(
        "{:30} {:02X?}, size: {}",
        "Send CBOR with COBS Frame:",
        send_frame,
        send_frame_size
    );
    info!("{}", msg);

    // 模擬 COBS 解碼
    let mut decoded_frame = vec![0; _cobs_size - 1]; // COBS 解碼後長度會減少
    let decoded_report = decode(&cobs_frame, &mut decoded_frame)?;
    let msg = format!(
        "{:30} {:02X?}, size: {}",
        "Decoded COBS Frame:",
        decoded_frame,
        decoded_report.frame_size()
    );
    info!("{}", msg);

    let decoded_message = Giga::decode_message(&decoded_frame)?;
    let msg = format!(
        "{:30} Action: {:?}, Command: {:?}, bSize: {:02X?}, bCRC: {:02X?}",
        "Decoded Message:",
        decoded_message.action,
        decoded_message.command,
        decoded_message.payload_size_bytes,
        decoded_message.crc_bytes
    );
    info!("{}", msg);
    let msg = format!(
        "{:30} {:02X?}, size: {}",
        "Decoded Payload Bytes:",
        decoded_message.payload_bytes,
        decoded_message.payload_size
    );
    info!("{}", msg);
    let msg = format!("{:30} {:?}", "Decoded Payload:", decoded_message.payload);
    info!("{}", msg);

    // let frame = build_frame(CMD::SEND, Command::MOTOR, &payload_cbor);
    // let dst_frame = frame.clone();
    // 2️⃣ 打開序列埠
    for port in serialport::available_ports()? {
        info!("\tFound port: {}", port.port_name);
    }
    let max_retries = 5; // 最大重試次數
    let mut giga = Giga::new(port_name, BAUD, timeout, max_retries, debug_mode, show_byte).await?;

    info!("成功打開序列埠: {}", port_name);
    // info!("等待 1 秒鐘...");
    // giga.send_cobs_motor(Action::SEND, Command::MOTOR).await?;

    // 4️⃣ 等待回覆
    info!("等待回覆...");
    giga.listen().await?;

    // ** Test Zone ** //
    // let mut payload = HashMap::new();
    // payload.insert("SRc", HashMap::new());
    // if let Some(val) = payload.get_mut("SRc") {
    //     val.insert("triggered".to_string(), true);
    // }
    // info!("{:?}", payload);
    // let payload_cbor: Vec<u8> = serde_cbor
    //     ::to_vec(&payload)
    //     .map_err(|e| anyhow::anyhow!("CBOR encode error: {}", e))?;
    // info!("CBOR 資料: {:02X?}", payload_cbor);
    // info!("CBOR 長度: {}", payload_cbor.len());
    // let (cobs_frame, _cobs_size, _crc) = GigaCommunicate::build_cobs_frame(
    //     Action::READ,
    //     Command::SensorHIGH,
    //     &payload_cbor
    // );
    // info!("Encoded Frame: {:02X?}", cobs_frame);
    // let mut send_cobs_frame = vec![0x00].into_iter().chain(cobs_frame).collect::<Vec<u8>>();
    // send_cobs_frame.push(0x00);
    // info!("Sending COBS Frame: {:02X?}, size: {}", send_cobs_frame, send_cobs_frame.len());

    // if let Some(val) = payload.get_mut("SRc") {
    //     if let Some(triggered) = val.get_mut("triggered") {
    //         *triggered = false;
    //     }
    // }
    // info!("{:?}", payload);
    // let payload_cbor: Vec<u8> = serde_cbor
    //     ::to_vec(&payload)
    //     .map_err(|e| anyhow::anyhow!("CBOR encode error: {}", e))?;
    // info!("CBOR 資料: {:02X?}", payload_cbor);
    // info!("CBOR 長度: {}", payload_cbor.len());
    // let (cobs_frame, _cobs_size, _crc) = GigaCommunicate::build_cobs_frame(
    //     Action::READ,
    //     Command::SensorHIGH,
    //     &payload_cbor
    // );
    // info!("Encoded Frame: {:02X?}", cobs_frame);
    // let mut send_cobs_frame = vec![0x00].into_iter().chain(cobs_frame).collect::<Vec<u8>>();
    // send_cobs_frame.push(0x00);
    // info!("Sending COBS Frame: {:02X?}, size: {}", send_cobs_frame, send_cobs_frame.len());

    Ok(())
}
