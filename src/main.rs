mod model;

use std::{ collections::HashMap, io::ErrorKind, time::Duration, vec };
// use tokio::time::sleep;
use anyhow::Error;
use cobs::{ encode, decode };
use crc::{ Crc, CRC_16_USB };
use serde_cbor::Value;
use model::{ Action, Command, Motion, StateMessage, Message };
#[allow(unused_imports)]
use tracing::{ info, error, debug, warn };
use tracing_subscriber::{
    fmt::{ self, format::FmtSpan },
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};
use tracing_appender::rolling;

const BAUD: u32 = 460_800;
const CRC16: Crc<u16> = Crc::<u16>::new(&CRC_16_USB);
const START_BYTE: [u8; 1] = [0x7e]; // 開始 byte
const MAX_DATA_LEN: usize = 1024;

async fn open_serial_port(
    port_name: &str,
    baud_rate: u32,
    timeout: Duration,
    max_retries: u32
) -> anyhow::Result<Box<dyn serialport::SerialPort>, Error> {
    let mut retries = 0;
    loop {
        match serialport::new(port_name, baud_rate).timeout(timeout).open() {
            Ok(port) => {
                return Ok(port);
            }
            Err(e) => {
                if retries >= max_retries {
                    eprintln!("無法打開序列埠 {}: {}", port_name, e);
                    return Err(anyhow::anyhow!("無法打開序列埠"));
                }
                retries += 1;
                eprintln!("無法打開序列埠 {}: {}, 正在重試第 {} 次", port_name, e, retries);
                // sleep(Duration::from_secs(1)).await;
                continue; // 重新嘗試打開序列埠
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ReceiveState {
    Normal,
    Debug,
    CheckingDebug(usize), // 參數表示已匹配的 DEBUG 字符數量
}

struct GigaCommunicate {
    port_name: String,
    baud_rate: u32,
    timeout: Duration,
    max_retries: u32,
    debug: bool,
    show_byte: bool,
    sensor_monitor: bool, // 是否啟用 Sensor Monitor 模式
    port: Box<dyn serialport::SerialPort>,

    buffer: [u8; MAX_DATA_LEN],
    buffer_receive_start_time: std::time::Instant,
    buffer_process_start_time: std::time::Instant,

    info_msg_buf: Vec<String>,
    debug_msg_buf: Vec<String>,
    idx: usize,
    len_bytes: [u8; 2],
    crc_bytes: [u8; 2],
    payload_size: usize,

    // Sensor State
    is_triggered: bool, // 是否已觸發
}

impl GigaCommunicate {
    pub async fn new(
        port_name: &str,
        baud_rate: u32,
        timeout: Duration,
        max_retries: u32,
        debug: bool,
        show_byte: bool,
        sensor_monitor: bool
    ) -> Result<Self, Error> {
        let port = match open_serial_port(port_name, baud_rate, timeout, max_retries).await {
            Ok(p) => p,
            Err(e) => {
                eprintln!("無法打開序列埠 {}: {}", port_name, e);
                return Err(anyhow::anyhow!("無法打開序列埠"));
            }
        };
        Ok(Self {
            port_name: port_name.to_string(),
            baud_rate,
            timeout,
            max_retries,
            debug,
            show_byte,
            sensor_monitor,
            port,
            buffer: [0u8; MAX_DATA_LEN],
            buffer_receive_start_time: std::time::Instant::now(),
            buffer_process_start_time: std::time::Instant::now(),
            idx: 0,
            len_bytes: [0u8; 2],
            crc_bytes: [0u8; 2],
            payload_size: 0,
            is_triggered: false, // 初始狀態未觸發
            info_msg_buf: Vec::new(),
            debug_msg_buf: Vec::new(),
        })
    }

    pub async fn reset(&mut self) {
        self.buffer = [0u8; MAX_DATA_LEN];
        self.idx = 0;
        self.len_bytes = [0u8; 2];
        self.crc_bytes = [0u8; 2];
        self.payload_size = 0;
        self.info_msg_buf.clear();
        self.debug_msg_buf.clear();
        // debug!("重置索引和 buffer");
    }

    pub async fn send_cobs_motor(&mut self, action: Action, command: Command) -> Result<(), Error> {
        let m1 = Motion {
            name: "PMt".into(),
            id: 5,
            motion: 1,
            speed: 100,
            tol: 5,
            dist: 2000,
            angle: 100,
            time: 5000,
            acc: 300,
            newid: 0,
            volt: 12.0,
            amp: 0.5,
            temp: 25.0,
            mode: 0,
        };
        let m2 = Motion {
            name: "PMb".into(),
            id: 4,
            motion: 1,
            speed: 100,
            tol: 2,
            dist: 1900,
            angle: 60,
            time: 4000,
            acc: 400,
            newid: 0,
            volt: 12.0,
            amp: 0.6,
            temp: 26.0,
            mode: 0,
        };
        let payload = vec![m1, m2];
        let msg = format!("{:30} {:?}", "Payload:", payload);
        debug!("{}", msg);

        let payload_cbor = serde_cbor::to_vec(&payload)?;
        let msg = format!("{:30} size={} {:02X?}", "CBOR:", payload_cbor.len(), payload_cbor);
        debug!("{}", msg);

        let (cobs_frame, cobs_size, crc) = Self::build_cobs_frame(action, command, &payload_cbor);
        let msg = format!("{:30} size={} crc={:02X?}", "COBS(CBOR):", cobs_size, crc);
        debug!("{}", msg);

        let mut send_cobs_frame = vec![0x00].into_iter().chain(cobs_frame).collect::<Vec<u8>>();
        send_cobs_frame.push(0x00);

        self.send(&send_cobs_frame)?;
        let msg = format!(
            "{:30} size={} {:02X?}",
            "Send COBS:",
            send_cobs_frame.len(),
            send_cobs_frame
        );
        debug!("{}", msg);
        Ok(())
    }

    pub async fn process_normal_byte(
        &mut self,
        byte: u8,
        buffer_started: &mut bool,
        receive_buf_elapsed_list: &mut Vec<Duration>,
        process_buf_elapsed_list: &mut Vec<Duration>
    ) -> Result<(), anyhow::Error> {
        self.buffer[self.idx] = byte;
        if self.buffer[self.idx] == 0x0d || self.buffer[self.idx] == 0x0a {
            // 忽略 CR 和 LF 字符
            // self.buffer[self.idx] = 0x00; // 將 CR 和 LF 替換為 0x00
        }
        match self.buffer[self.idx] {
            0x00 => {
                if !*buffer_started {
                    // 第一個 0x00 字節表示開始接收資料
                    self.buffer_receive_start_time = std::time::Instant::now();
                    *buffer_started = true; // 標記已經開始接收資料
                } else if self.buffer[0..self.idx].len() > 0 {
                    // 第二個 0x00 字節表示結束接收資料
                    let receive_elapsed = self.buffer_receive_start_time.elapsed();
                    receive_buf_elapsed_list.push(receive_elapsed);
                    if receive_buf_elapsed_list.len() > 100 {
                        receive_buf_elapsed_list.remove(0); // 保持列表長度不超過 100
                    }
                    // 平均接收資料耗時
                    let avg_receive_elapsed =
                        receive_buf_elapsed_list.iter().sum::<Duration>() /
                        (receive_buf_elapsed_list.len() as u32);

                    // 開始處理資料
                    self.buffer_process_start_time = std::time::Instant::now();
                    let cobs_buffer = &self.buffer[0..self.idx];
                    let msg = format!(
                        "{:30} size={} {:02X?}",
                        "Received COBS:",
                        cobs_buffer.len(),
                        cobs_buffer
                    );
                    debug!("{}", msg);

                    // 處理 COBS Frame
                    let mut decoded_frame = vec![0; cobs_buffer.len() - 1]; // COBS 解碼後長度會減少
                    let decoded_report = decode(cobs_buffer, &mut decoded_frame).map_err(|e| {
                        eprintln!("COBS decode error: {}", e);
                        anyhow::anyhow!("COBS decode error: {}", e)
                    })?;
                    let msg = format!(
                        "{:30} size={} {:02X?}",
                        "Decoded COBS:",
                        decoded_report.frame_size(),
                        decoded_frame
                    );
                    debug!("{}", msg);

                    let decoded_message = Self::decode_message(&decoded_frame)?;

                    // 資料處理耗時
                    let buffer_process_elapsed = self.buffer_process_start_time.elapsed();
                    let msg = format!(
                        "{:30} bSize={:02X?} bCRC={:02X?} bPayload={:02X?}",
                        "Decoded COBS Bytes:",
                        decoded_message.payload_size_bytes,
                        decoded_message.crc_bytes,
                        decoded_message.payload_bytes
                    );
                    debug!("{}", msg);
                    let msg = format!(
                        "{:30} size={} Action={:?} Command={:?} Payload={:?}",
                        "Decoded Message:",
                        decoded_message.payload_size,
                        decoded_message.action,
                        decoded_message.command,
                        decoded_message.payload
                    );
                    debug!("{}", msg);

                    process_buf_elapsed_list.push(buffer_process_elapsed);
                    if process_buf_elapsed_list.len() > 100 {
                        process_buf_elapsed_list.remove(0); // 保持列表長度不超過 100
                    }

                    // 平均資料處理耗時
                    let avg_buffer_process_elapsed =
                        process_buf_elapsed_list.iter().sum::<Duration>() /
                        (process_buf_elapsed_list.len() as u32);

                    let msg = format!(
                        "{:28} 收資料: {:>9}, 平均收資料: {:>9}, 資料處理: {:>9}, 平均資料處理: {:>9}",
                        "耗時:",
                        format!("{:.2?}", receive_elapsed),
                        format!("{:.2?}", avg_receive_elapsed),
                        format!("{:.2?}", buffer_process_elapsed),
                        format!("{:.2?}", avg_buffer_process_elapsed)
                    );
                    debug!("{}", msg);

                    let mut update_sensor_trigger = false;
                    if
                        decoded_message.command == Command::Sensor ||
                        decoded_message.command == Command::SensorLOW
                    {
                        // Old Ver.: 0x06 Triggered, 0x07 Not Triggered
                        // New Ver.: 0x06 payload: {"name": "trigger_1", "triggered": true}, 0x06 payload: {"name":"trigger_2", "triggered": false}, HIGH: false, LOW: true
                        // 根據 payload 判斷是否觸發
                        let triggered_value = decoded_message.payload
                            .get("triggered")
                            .unwrap_or(&Value::Null);
                        if let Value::Bool(triggered) = triggered_value {
                            self.is_triggered = *triggered;
                        } else {
                            let mut is_motor_triggered_state = false;
                            for motor_name in decoded_message.payload.keys() {
                                if
                                    let Some(motor_trigger_state) =
                                        decoded_message.payload.get(motor_name)
                                {
                                    if let Value::Map(motor_triggered_value) = motor_trigger_state {
                                        if
                                            let Value::Bool(triggered) = motor_triggered_value
                                                .get(&Value::Text("triggered".to_string()))
                                                .unwrap_or(&Value::Null)
                                        {
                                            self.is_triggered = *triggered;
                                            is_motor_triggered_state = true;
                                            update_sensor_trigger = true;
                                        } else {
                                            warn!(
                                                "Motor Payload does not contain 'triggered' key or is not a boolean"
                                            );
                                            break;
                                        }
                                    }
                                }
                            }

                            if !is_motor_triggered_state {
                                warn!(
                                    "Motor Payload does not contain 'triggered' key or is not a boolean"
                                );
                                if decoded_message.command == Command::Sensor {
                                    self.is_triggered = false;
                                } else {
                                    self.is_triggered = true;
                                }
                            }
                        }
                        self.send_cobs_motor(Action::SEND, Command::MOTOR).await?;
                    }
                    if update_sensor_trigger || self.debug {
                        let msg = format!("{:30} {}", "Sensor Is Triggered:", self.is_triggered);
                        info!("{}", msg);

                        let msg = "=".repeat(80);
                        info!("{}", msg);
                    }
                    *buffer_started = false; // 重置標記
                }
                self.reset().await;
            }
            _ => {
                self.idx += 1;
            }
        }

        if self.idx >= MAX_DATA_LEN {
            debug!("Buffer overflow, resetting...");
            self.reset().await;
        }

        Ok(())
    }

    #[allow(unreachable_code)]
    pub async fn listen(&mut self) -> Result<(), Error> {
        let mut buffer_started = false; // 標記是否已經開始接收資料
        let mut receive_buf_elapsed_list = Vec::<Duration>::new(); // 用於存儲資料接收耗時
        let mut process_buf_elapsed_list = Vec::<Duration>::new(); // 用於存儲資料處理耗時

        let debug_sequence = b"[DEBUG]";
        let mut receive_state = ReceiveState::Normal;
        let mut debug_output = String::new();

        loop {
            let mut buf = [0u8; 1];
            let read_result = self.port.read(&mut buf);

            match read_result {
                Ok(_) => {
                    let received_byte = buf[0];
                    match receive_state {
                        ReceiveState::Normal => {
                            if self.debug && self.show_byte {
                                debug!("byte[{}]: {:02X}", self.idx, received_byte);
                            }

                            if received_byte == debug_sequence[0] {
                                receive_state = ReceiveState::CheckingDebug(1);
                            } else {
                                self.process_normal_byte(
                                    received_byte,
                                    &mut buffer_started,
                                    &mut receive_buf_elapsed_list,
                                    &mut process_buf_elapsed_list
                                ).await?;
                            }
                        }
                        ReceiveState::CheckingDebug(match_count) => {
                            if
                                match_count < debug_sequence.len() &&
                                received_byte == debug_sequence[match_count]
                            {
                                if match_count + 1 == debug_sequence.len() {
                                    // 完整匹配到 DEBUG
                                    receive_state = ReceiveState::Debug;
                                    // info!("進入 DEBUG 狀態");
                                } else {
                                    receive_state = ReceiveState::CheckingDebug(match_count + 1);
                                }
                            } else {
                                // 匹配失敗，回到正常模式並處理之前的字符
                                receive_state = ReceiveState::Normal;
                                // 處理之前的字符
                                for i in 0..match_count {
                                    self.process_normal_byte(
                                        debug_sequence[i],
                                        &mut buffer_started,
                                        &mut receive_buf_elapsed_list,
                                        &mut process_buf_elapsed_list
                                    ).await?;
                                }
                                // 處理當前字符
                                self.process_normal_byte(
                                    received_byte,
                                    &mut buffer_started,
                                    &mut receive_buf_elapsed_list,
                                    &mut process_buf_elapsed_list
                                ).await?;
                            }
                        }
                        ReceiveState::Debug => {
                            if received_byte == b'\n' || received_byte == b'\r' {
                                if debug_output.contains("CBOR Motor Receiver Ready") {
                                    // self.send_cobs_motor(Action::READ, Command::MOTOR).await?;
                                }
                                debug!("{:30} {}", format!("Giga:"), debug_output);
                                debug_output.clear();
                                receive_state = ReceiveState::Normal;
                            } else if received_byte == 0x1b {
                                // ESC 鍵退出 DEBUG 模式
                                receive_state = ReceiveState::Normal;
                                // info!("離開 DEBUG 模式");
                                debug_output.clear();
                            } else if received_byte >= 0x20 && received_byte <= 0x7e {
                                debug_output.push(received_byte as char);
                            }
                        }
                    }
                }
                Err(e) => {
                    match e.kind() {
                        ErrorKind::TimedOut => {
                            // timeout 正常
                            if !self.sensor_monitor {
                                warn!("讀取串口資料超時，傳送資料並繼續等待回覆...");
                                self.send_cobs_motor(Action::READ, Command::MOTOR).await?;
                            } else {
                                warn!("等待 Sensor 資料...");
                            }
                            continue;
                        }
                        _ => {
                            debug!("讀取串口資料失敗，可能是串口已關閉或發生錯誤");
                            // 嘗試關閉並重新打開串口
                            debug!("關閉序列埠: {}", self.port_name);
                            // 嘗試重新打開串口
                            self.port = match
                                open_serial_port(
                                    &self.port_name,
                                    self.baud_rate,
                                    self.timeout,
                                    self.max_retries
                                ).await
                            {
                                Ok(p) => p,
                                Err(e) => {
                                    let msg = format!(
                                        "無法重新打開序列埠 {}: {}",
                                        self.port_name,
                                        e
                                    );
                                    error!("{}", msg);
                                    return Err(anyhow::anyhow!(msg));
                                }
                            };
                            debug!("重新打開序列埠: {}", self.port_name);
                            continue; // 重新開始循環
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn build_frame(action: Action, command: Command, payload: &[u8]) -> (Vec<u8>, u16) {
        let mut frame = Vec::with_capacity(payload.len() + 7);
        // 開始 byte
        frame.extend(START_BYTE); // 1 byte
        // Action Byte, SEND: 0xAA, READ: 0xA8
        frame.push(action as u8); // 1 byte
        // Command Byte, Ack=0x01, Nack=0x02, Motor=0x03, SetID=0x04, File=0x05, Sensor High=0x06, Sensor Low=0x07
        frame.push(command as u8); // 1 byte
        let len = payload.len() as u16;
        frame.extend(len.to_le_bytes()); // 2 bytes
        frame.extend(payload); // payload 長度可變
        // 跳過 START_BYTE, Action Byte, Command Byte
        let crc = CRC16.checksum(&frame[3..]);
        // lo, hi  // 2 bytes
        frame.extend(crc.to_le_bytes());
        (frame, crc)
    }

    pub fn build_cobs_frame(
        action: Action,
        command: Command,
        payload: &[u8]
    ) -> (Vec<u8>, usize, u16) {
        let default_size = 1 + 1 + 2 + 2; // Action + Command + Length + CRC
        let crc_skip_bytes = 2; // 跳過 Action 和 Command Bytes
        let mut frame = Vec::with_capacity(payload.len() + default_size);
        // 1 byte, Action Byte, SEND: 0xAA, READ: 0xA8
        frame.push(action as u8);
        // 1 byte, Command Byte, Ack=0x01, Nack=0x02, Motor=0x03, SetID=0x04, File=0x05, Sensor High=0x06, Sensor Low=0x07
        frame.push(command as u8);
        // 2 bytes, Length, Payload 長度
        let len = payload.len() as u16;
        frame.extend(len.to_le_bytes());
        // n bytes, Payload, 可變長度
        frame.extend(payload);
        // 2 bytes, CRC 計算, 跳過 Action, Command Bytes
        let crc = CRC16.checksum(&frame[crc_skip_bytes..]);
        frame.extend(crc.to_le_bytes());

        // COBS 編碼
        let mut encoded_frame = vec![0; frame.len() + 1]; // COBS 編碼後長度會增加
        let encoded_size = encode(&frame, &mut encoded_frame);

        (encoded_frame, encoded_size, crc)
    }

    pub fn decode_message(frame: &[u8]) -> Result<Message, Error> {
        let frame_size = frame.len();
        if frame_size < 6 {
            // 最小長度為 6 bytes, 包含 Action Byte, Command Byte, Length, CRC
            let msg = format!("Frame too short: expected at least 6 bytes, got {}", frame_size);
            error!("{}", msg);
            return Err(anyhow::anyhow!("{}", msg));
        }
        // 1 byte, Action Byte
        let action = Action::try_from(frame[0]).unwrap_or(Action::NONE);
        // 1 byte, Command Byte
        let command = Command::try_from(frame[1]).unwrap_or(Command::NONE);
        // 2 bytes, Length
        let payload_size_bytes = [frame[2], frame[3]];
        let payload_size = u16::from_le_bytes(payload_size_bytes);
        // n bytes, Payload
        let payload_bytes = &frame[4..4 + (payload_size as usize)];
        let payload = serde_cbor::from_slice::<HashMap<String, Value>>(payload_bytes).map_err(|e| {
            let msg = format!("CBOR decode error: {}", e);
            error!("{}", msg);
            anyhow::anyhow!("{}", msg)
        })?;
        // 2 bytes, CRC
        let crc_bytes = &frame[frame_size - 2..frame_size];
        let crc = u16::from_le_bytes(crc_bytes.try_into().unwrap());
        let calc_crc = CRC16.checksum(&frame[2..frame_size - 2]);
        if crc != calc_crc {
            let msg = format!("CRC mismatch: expected {:04X}, got {:04X}", calc_crc, crc);
            error!("{}", msg);
            return Err(anyhow::anyhow!("{}", msg));
        }
        Ok(Message {
            action,
            command,
            payload_size_bytes: vec![frame[2], frame[3]],
            payload_size,
            payload_bytes: payload_bytes.to_vec(),
            payload,
            crc_bytes: crc_bytes.to_vec(),
            crc,
        })
    }

    pub fn send(&mut self, frame: &[u8]) -> Result<(), Error> {
        // debug!("傳送資料: {:02X?}", frame);
        self.port.write_all(frame)?;
        self.port.flush()?;
        // debug!("資料傳送成功");
        Ok(())
    }
}

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
    let (frame, _crc) = GigaCommunicate::build_frame(Action::READ, Command::Sensor, &payload_cbor);
    let msg = format!(
        "{:30} {:02X?}, len: {}, {:02X?}, CRC: {:02X?}",
        "Send CBOR without COBS Frame:",
        frame,
        frame.len(),
        (frame.len() as u16).to_le_bytes(),
        _crc.to_le_bytes()
    );
    info!("{}", msg);

    // 建立 COBS 編碼的 frame
    let (cobs_frame, _cobs_size, crc) = GigaCommunicate::build_cobs_frame(
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

    let decoded_message = GigaCommunicate::decode_message(&decoded_frame)?;
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
    let mut giga = GigaCommunicate::new(
        port_name,
        BAUD,
        timeout,
        max_retries,
        debug_mode,
        show_byte,
        sensor_monitor
    ).await?;

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
