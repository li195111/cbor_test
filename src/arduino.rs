use std::{ collections::HashMap, io::ErrorKind, time::Duration, vec };
use anyhow::Error;
use cobs::{ encode, decode };
use crc::{ Crc, CRC_16_USB };
use serde_cbor::Value;
#[allow(unused_imports)]
use tracing::{ info, error, debug, warn };

use crate::{
    model::{ Action, Command, Message, Motion, ReceiveState },
    serial::{ open_serial_port },
};

pub const BAUD: u32 = 460_800;
pub const CRC16: Crc<u16> = Crc::<u16>::new(&CRC_16_USB);
pub const START_BYTE: [u8; 1] = [0x7e]; // 開始 byte
pub const MAX_DATA_LEN: usize = 1024;

pub struct Giga {
    /// 序列埠名稱
    port_name: String,

    /// 波特率
    baud_rate: u32,

    /// 超時時間
    timeout: Duration,

    /// 最大重試次數
    max_retries: u32,

    /// 除錯模式
    debug: bool,

    /// 顯示原始資料
    show_byte: bool,

    /// 是否啟用 Sensor Monitor 模式
    sensor_monitor: bool,

    /// 序列埠
    port: Box<dyn serialport::SerialPort>,

    /// 接收緩衝區
    buffer: [u8; MAX_DATA_LEN],
    /// 接收開始時間
    buffer_receive_start_time: std::time::Instant,
    /// 接收處理開始時間
    buffer_process_start_time: std::time::Instant,

    /// 資訊訊息緩衝區
    info_msg_buf: Vec<String>,
    /// 除錯訊息緩衝區
    debug_msg_buf: Vec<String>,
    /// 當前索引
    idx: usize,
    /// 長度 bytes
    len_bytes: [u8; 2],
    /// CRC bytes
    crc_bytes: [u8; 2],
    /// Payload size
    payload_size: usize,

    // Sensor State
    is_triggered: bool, // 是否已觸發
}

impl Giga {
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
            // CR 和 LF 字符處理
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
                            debug!("讀取串口資料失敗，可能是串口已關閉或發生錯誤: {}", e);
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
                            continue;
                        }
                    }
                }
            }
        }
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
