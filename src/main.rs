use std::{
    collections::HashMap,
    sync::{ atomic::{ AtomicBool, Ordering }, Arc },
    time::{ Duration, Instant },
    vec,
    io::{ self, Write },
};

use configparser::ini::Ini;
use cobs::{ decode };
use serde::{ Serialize, Deserialize };
use tokio::{ io::BufReader, sync::{ mpsc, Mutex } };
use serde_json::Value;
use tracing::*;
use tracing_subscriber::{ fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter };
use tracing_appender::rolling;

use pingpong_arduino::{
    build_cobs_frame, decode_message, Action, Command, Giga, SensorConfig, StateMessage, DEFAULT_BAUDRATE
};

static LAST_GIGA_LOG: std::sync::OnceLock<std::sync::Mutex<Instant>> = std::sync::OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetMotorPayload {
    pub id: u8,
    pub motion: u8,
    pub rpm: i64,
    pub acc: u64,
    pub volt: f32,
    pub temp: f32,
    pub amp: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Payload {
    Set(HashMap<String, SetMotorPayload>),
    Read(HashMap<String, Value>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MotorCommandParams {
    pub action: Action,
    pub cmd: Command,
    pub payload: Payload,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let buardrate = DEFAULT_BAUDRATE;
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

    let timeout = kwargs
        .get("timeout")
        .and_then(|v| v.parse::<f64>().ok())
        .map_or(Duration::from_secs_f64(0.0005), Duration::from_secs_f64);
    let show_giga = kwargs.get("show_giga").map_or(false, |v| (v == "true" || v == "1"));
    let show_giga_interval = kwargs
        .get("show_giga_interval")
        .and_then(|v| v.parse::<f64>().ok())
        .map_or(Duration::from_secs_f64(0.1), Duration::from_secs_f64);

    let dir_name = "logs";
    let file_name = "cbor_test.log";
    // 1. 準備檔案 appender（logs/YYYY-MM-DD.log）
    let file_app = rolling::daily(dir_name, file_name);
    let (file_writer, guard) = tracing_appender::non_blocking(file_app);

    // 2. 建 stdout layer
    let stdout_layer = fmt
        ::layer()
        .with_writer(std::io::stdout) // 終端輸出
        // .without_time() // 不印時間
        .with_target(false) // 不印 module 名
        .with_file(false) // 顯示檔案名稱
        .with_line_number(false) // 顯示行號
        .with_thread_ids(true) // 顯示線程 ID
        .with_ansi(true); // 顯示顏色

    // 3. 建 file layer
    let file_layer = fmt
        ::layer()
        .with_writer(file_writer) // 背景 thread 寫檔
        .with_target(false) // 顯示模組路徑（target）
        .with_file(true) // 顯示檔案名稱
        .with_line_number(true) // 顯示行號
        .with_thread_ids(true) // 顯示線程 ID
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

    info!("ℹ️ CBOR Test 開始 ================================================================");
    info!("{} {}", format!("{:<30}", "Use Serial Port:"), port_name);
    info!("{} {}", format!("{:<30}", "Use Baud Rate:"), buardrate);
    info!("{} {}", format!("{:<30}", "DEBUG Mode:"), debug_mode);
    info!("{} {:?}", format!("{:<30}", "Timeout:"), timeout);
    info!("{} {}", format!("{:<30}", "Show Byte:"), show_byte);
    info!("{} {}", format!("{:<30}", "Show Giga Message:"), show_giga);
    info!("{} {:?}", format!("{:<30}", "Show Giga Message Interval(sec.):"), show_giga_interval);

    info!("ℹ️ Payload Test ================================================================");
    // payload
    let payload = StateMessage { status: 0 };
    info!("{:30} {}, size: {}", "PayLoad:", payload, std::mem::size_of_val(&payload));

    // 將 payload 序列化為 CBOR 格式
    let payload_cbor = serde_cbor::to_vec(&payload)?;
    info!("{:30} {:02X?}, size: {}", "PayLoad CBOR:", payload_cbor, payload_cbor.len());

    // 建立要傳送的 frame
    let (frame, cobs_size, crc) = build_cobs_frame(Action::READ, Command::Sensor, &payload_cbor);
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
    let (cobs_frame, _cobs_size, crc) = build_cobs_frame(
        Action::SEND,
        Command::Motor,
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

    let decoded_message = decode_message(&decoded_frame)?;
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
    info!(
        "🎉 Payload Test Complete ================================================================"
    );

    info!(
        "ℹ️ Giga Connection Test ================================================================"
    );
    info!("ℹ️ Search Serial Ports:");
    // 2️⃣ 打開序列埠
    for port in serialport::available_ports()? {
        info!("\tFound port: {}", port.port_name);
    }

    let exit_flag = Arc::new(AtomicBool::new(false));
    let is_giga_connected = Arc::new(AtomicBool::new(false));

    let (giga_send_tx, mut giga_send_rx) = mpsc::channel::<MotorCommandParams>(128);
    let (giga_reconnect_tx, mut giga_reconnect_rx) = mpsc::channel::<bool>(128);

    let mut config = Ini::new();
    config.set("DEFAULT", "LEGACY", Some("false".to_string()));
    config.set("SENSOR.WINDOWS", "PORT", Some(port_name.to_string()));
    config.set("SENSOR.UNIX", "PORT", Some(port_name.to_string()));
    config.set("SENSOR", "TRIGGER_TIMEOUT", Some("2".to_string()));
    config.set("SENSOR", "BAUDRATE", Some(buardrate.to_string()));
    config.set("SENSOR", "TIMEOUT", Some(timeout.as_secs_f64().to_string()));
    config.set("DEFAULT", "DEBUG", Some(debug_mode.to_string()));

    let sensor_config = Arc::new(Mutex::new(SensorConfig::new(config).await?));

    let mut giga_opt = Giga::connection(
        show_byte,
        &sensor_config,
        &is_giga_connected,
        move |msg| {
            if msg.action != Action::GIGA {
                info!("{} Message Resp: {:?}", msg.action, msg);
            } else if show_giga {
                let now = Instant::now();
                let lock = LAST_GIGA_LOG.get_or_init(||
                    std::sync::Mutex::new(now - show_giga_interval)
                );
                let mut last = lock.lock().unwrap();
                if now.duration_since(*last) >= show_giga_interval {
                    *last = now;
                    info!("{} Message Recv: {:?}", msg.action, msg.payload);
                }
            }
        },
        move |msg| {
            info!("Send CBOR: {} {:?}", msg.len(), msg);
        },
        move |msg| {
            info!("Send COBS: {} {:?}", msg.len(), msg);
        }
    ).await;

    // info!("ℹ️ 成功打開序列埠: {}", port_name);
    // // 4️⃣ 等待回覆
    // info!("⏳ 等待回覆...");

    let sample_json = format!(
        "{{\"action\": \"SEND\", \"cmd\": \"Motor\", \"payload\": {{\"PMt\": {{  \"id\": 1,  \"motion\": 1,  \"rpm\": 500,  \"acc\": 0,  \"volt\": 0,  \"temp\": 0,  \"amp\": 0 }}}}}}"
    );
    info!("🔔 Use 'q' or '/q' to Exit program");
    info!("🔔 Use 'show_giga=true' to Show Giga Message");
    info!("🔔 Use 'show_giga_interval' to Set Giga Message Interval");
    info!("🔔 Use '/t=N' to Send N times of Motor Payload");
    info!("🔔 Use '/r' to Reconnect the Giga");
    info!("🔔 Sample JSON: {}", sample_json);
    info!(
        "🔔 {} {}, {}, {}",
        format!("{:<30}", "Action:"),
        Action::SEND,
        Action::READ,
        Action::GIGA
    );
    info!(
        "🔔 {} {}, {}, {}, {}, {}",
        format!("{:<30}", "Cmd:"),
        Command::Ack,
        Command::NAck,
        Command::Motor,
        Command::Sensor,
        Command::File
    );
    // 移交唯一的 Arc<Giga> 到背景任務，避免多重 Arc 使 Arc::get_mut 失效
    let exit_flag_clone = exit_flag.clone();
    tokio::task::spawn(async move {
        let mut is_first_log = true;
        let mut previous_triggered_count = 0;
        loop {
            while let Ok(reconnect) = giga_reconnect_rx.try_recv() {
                if reconnect {
                    giga_opt = Giga::reconnect(
                        show_byte,
                        &sensor_config,
                        &is_giga_connected,
                        move |msg| {
                            if msg.action != Action::GIGA {
                                info!("{} Message Resp: {:?}", msg.action, msg);
                            } else if show_giga {
                                let now = Instant::now();
                                let lock = LAST_GIGA_LOG.get_or_init(||
                                    std::sync::Mutex::new(now - show_giga_interval)
                                );
                                let mut last = lock.lock().unwrap();
                                if now.duration_since(*last) >= show_giga_interval {
                                    *last = now;
                                    info!("{} Message Recv: {:?}", msg.action, msg.payload);
                                }
                            }
                        },
                        move |msg| {
                            info!("Send CBOR: {} {:?}", msg.len(), msg);
                        },
                        move |msg| {
                            info!("Send COBS: {} {:?}", msg.len(), msg);
                        }
                    ).await;
                }
            }

            let current_triggered_count = if is_giga_connected.load(Ordering::Acquire) {
                if let Some(ref mut giga_arc) = giga_opt {
                    if let Some(giga_inner) = Arc::get_mut(giga_arc) {
                        match giga_inner.listen_once().await {
                            Ok(_) => {}
                            Err(e) => {
                                giga_inner.exit_flag.store(true, Ordering::Release);
                                debug!("Giga Listen Error, connection lost: {}", e);
                                is_giga_connected.store(false, Ordering::Release);
                                warn!("Giga connection lost, use `/r` or `r` to reconnect");
                                print!("\n>> ");
                                io::stdout().flush().unwrap();
                                continue;
                            }
                        }

                        while let Ok(send_msg) = giga_send_rx.try_recv() {
                            if
                                let Err(e) = giga_inner.send_cobs_object(
                                    send_msg.payload,
                                    send_msg.action,
                                    send_msg.cmd
                                ).await
                            {
                                error!("Failed to send Giga Data: {}", e);
                            }
                        }
                    }
                    giga_arc.triggered_counts.load(Ordering::Acquire)
                } else {
                    0
                }
            } else {
                0
            };

            let triggered = current_triggered_count != previous_triggered_count;

            if triggered && is_first_log {
                {
                    // Do something
                }
                is_first_log = false;
            } else if !triggered && !is_first_log {
                is_first_log = true;
            }

            if exit_flag_clone.load(Ordering::Acquire) {
                info!("========== Giga Exiting ==========");
                // 直接對內部的 Giga 設定退出旗標
                if is_giga_connected.load(Ordering::Acquire) {
                    if let Some(giga_arc) = giga_opt {
                        giga_arc.exit_flag.store(true, Ordering::Release);
                    }
                }
                info!("========== Giga Stop ==========");
                break;
            }
            previous_triggered_count = current_triggered_count;
            tokio::task::yield_now().await;
        }
    });
    let tag_list = [
        "id",
        "motion",
        "rpm",
        "tol",
        "dist",
        "angle",
        "time",
        "acc",
        "newid",
        "volt",
        "amp",
        "temp",
        "mode",
        "status",
    ];
    let test_motor_name = "PMt";
    let mut test_json_str;
    loop {
        let mut input = String::new();

        print!("\n>> ");
        io::stdout().flush().unwrap();

        // 建立一個非同步的 BufReader 來讀取 stdin
        let mut reader = BufReader::new(tokio::io::stdin());
        let n = tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut input).await?;
        if n == 0 {
            continue;
        }
        let mut line = input.trim();
        if line.is_empty() {
            continue;
        }
        if line.eq_ignore_ascii_case("/q") || line.eq_ignore_ascii_case("q") {
            exit_flag.store(true, Ordering::Release);
            tokio::time::sleep(Duration::from_millis(100)).await;
            break;
        }
        if line.eq_ignore_ascii_case("/r") || line.eq_ignore_ascii_case("r") {
            if let Err(e) = giga_reconnect_tx.send(true).await {
                error!("Failed to send reconnect signal: {}", e);
            }
            continue;
        }
        line = if line.starts_with("/t=") {
            let n = line.trim_start_matches("/t=");
            if let Ok(test_count) = n.parse::<u64>() {
                let mut payload_map = serde_json::Map::new();
                for i in 0..test_count {
                    let k = if i > 0 {
                        format!("{}{}", test_motor_name, i)
                    } else {
                        test_motor_name.to_string()
                    };
                    let v = tag_list.to_vec();
                    payload_map.insert(k, v.into());
                }
                let mut test_json_map = serde_json::Map::new();
                test_json_map.insert("action".to_string(), "READ".into());
                test_json_map.insert("cmd".to_string(), "Motor".into());
                test_json_map.insert("payload".to_string(), serde_json::Value::Object(payload_map));
                test_json_str = serde_json::to_string(&test_json_map).unwrap();
                info!("Generated test JSON: {}", test_json_str);
                test_json_str.as_str()
            } else {
                error!("Invalid test count value: {}", n);
                line
            }
        } else {
            line
        };

        match serde_json::from_str::<MotorCommandParams>(line) {
            Ok(cmd) => {
                info!("Received: {:?}", cmd);
                if let Err(e) = giga_send_tx.send(cmd).await {
                    error!("Failed to enqueue command: {}", e);
                } else {
                    info!("Command queued");
                }
            }
            Err(e) => {
                error!("Invalid JSON: {}", e);
            }
        }
        tokio::task::yield_now().await;
    }
    info!("🎊 CBOR Test Complete ================================================================");
    Ok(())
}
