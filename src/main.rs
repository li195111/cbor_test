mod model;

use std::{ collections::HashMap, thread::sleep, time::Duration, vec };
use anyhow::Error;
#[allow(unused_imports)]
use cobs::{ encode, decode };
use crc::{ Crc, CRC_16_USB };
use serde_cbor::Value;
use model::{ Action, Command, Motion, StateMessage, Message };

const BAUD: u32 = 460_800;
const CRC16: Crc<u16> = Crc::<u16>::new(&CRC_16_USB);
const START_BYTE: [u8; 1] = [0x7e]; // 開始 byte
const MAX_DATA_LEN: usize = 1024;

fn open_serial_port(
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
                sleep(Duration::from_secs(1));
                continue; // 重新嘗試打開序列埠
            }
        }
    }
}

struct GigaCommunicate {
    port_name: String,
    baud_rate: u32,
    timeout: Duration,
    max_retries: u32,
    port: Box<dyn serialport::SerialPort>,
    pub messages: Vec<String>,

    pub buffer: [u8; MAX_DATA_LEN],
    pub idx: usize,
    pub len_bytes: [u8; 2],
    pub crc_bytes: [u8; 2],
    pub payload_size: usize,
}

impl GigaCommunicate {
    pub fn new(
        port_name: &str,
        baud_rate: u32,
        timeout: Duration,
        max_retries: u32
    ) -> Result<Self, Error> {
        let port = match open_serial_port(port_name, baud_rate, timeout, max_retries) {
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
            port,
            buffer: [0u8; MAX_DATA_LEN],
            idx: 0,
            len_bytes: [0u8; 2],
            crc_bytes: [0u8; 2],
            payload_size: 0,
            messages: Vec::new(),
        })
    }

    pub async fn reset(&mut self) {
        // if self.idx > 3 {
        //     println!("重設: idx: {}, buffer: {:02X?}", self.idx, &self.buffer[..self.idx + 1]);
        //     for message in &self.messages {
        //         println!("\t{}", message);
        //     }
        // }
        self.buffer = [0u8; MAX_DATA_LEN];
        self.idx = 0;
        self.len_bytes = [0u8; 2];
        self.crc_bytes = [0u8; 2];
        self.payload_size = 0;
        self.messages.clear();
        self.messages.push("重置索引和 buffer".into());
    }

    pub async fn send_motor(&mut self, action: Action, command: Command) -> Result<(), Error> {
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
        let payload: Vec<Motion> = vec![m1, m2]; // 對應外層 3 元素陣列
        self.messages.push(format!("Payload: {:?}", payload));
        let payload_cbor: Vec<u8> = serde_cbor
            ::to_vec(&payload)
            .map_err(|e| anyhow::anyhow!("CBOR encode error: {}", e))?;
        self.messages.push(format!("CBOR 資料: {:02X?}", payload_cbor));
        self.messages.push(format!("CBOR 長度: {}", payload_cbor.len()));
        let (frame, _crc) = Self::build_frame(action, command, &payload_cbor);
        self.send(&frame)?;
        Ok(())
    }

    async fn process_frame(
        &mut self,
        times: &mut i32,
        action: &mut Action,
        command: &mut Command,
        start_time: &mut std::time::Instant,
        elapsed_list: &mut Vec<Duration>
    ) -> Result<u8, Error> {
        match self.idx {
            0 => {
                if self.buffer[self.idx] != START_BYTE[0] {
                    self.reset().await;
                    return Ok(0);
                }
                self.messages.push(format!("收到 Start Byte: {:02X?}", self.buffer[self.idx]));
                self.idx += 1;
                return Ok(0); // 返回 1 表示已經處理了 Start Byte
            }
            1 => {
                if let Ok(act) = Action::try_from(self.buffer[self.idx]) {
                    *action = act;
                    self.messages.push(format!("收到 Action Byte: {:02X?}", *action as u8));
                    self.idx += 1;
                    return Ok(0); // 返回 0 表示已經處理了 Action Byte
                } else {
                    self.reset().await;
                    return Ok(0);
                }
            }
            2 => {
                if let Ok(cmd) = Command::try_from(self.buffer[self.idx]) {
                    *command = cmd;
                    self.messages.push(format!("收到 Command Byte: {:02X?}", *command as u8));
                    self.idx += 1;
                    return Ok(0);
                } else {
                    self.reset().await;
                    return Ok(0);
                }
            }
            3 => {
                self.len_bytes[0] = self.buffer[self.idx];
                self.idx += 1;
                return Ok(0);
            }
            4 => {
                self.len_bytes[1] = self.buffer[self.idx];
                self.payload_size = u16::from_le_bytes(self.len_bytes) as usize;
                if self.payload_size > MAX_DATA_LEN - 7 {
                    return Err(anyhow::anyhow!("Payload size exceeds maximum limit"));
                }
                self.messages.push(format!("Payload Size: {} bytes", self.payload_size));
                self.idx += 1;
                return Ok(0);
            }
            idx if idx >= 5 && idx < 5 + self.payload_size => {
                // 收到 payload 資料
                self.idx += 1;
                return Ok(0);
            }
            idx if idx >= 5 + self.payload_size && idx < 7 + self.payload_size => {
                // 收到 CRC 資料
                self.crc_bytes[self.idx - (5 + self.payload_size)] = self.buffer[self.idx];
                if self.idx == 6 + self.payload_size {
                    let crc = u16::from_le_bytes(self.crc_bytes);
                    let calculated_crc = CRC16.checksum(&self.buffer[3..5 + self.payload_size]);
                    if crc != calculated_crc {
                        return Err(
                            anyhow::anyhow!(
                                "CRC mismatch: expected {:04X}, got {:04X}",
                                calculated_crc,
                                crc
                            )
                        );
                    }
                    self.messages.push("接收資料完成，CRC 校驗成功".into());
                    let decoded_payload = serde_cbor
                        ::from_slice::<HashMap<String, Value>>(
                            &self.buffer[5..5 + self.payload_size]
                        )
                        .map_err(|e| anyhow::anyhow!("CBOR decode error: {}", e))?;
                    let elapsed = start_time.elapsed();
                    elapsed_list.push(elapsed);
                    let avg_elapsed =
                        elapsed_list.iter().sum::<Duration>() / (elapsed_list.len() as u32);
                    let message = format!(
                        "耗時: {:>9}, 次數: {}, 平均耗時: {:>9}, 接收到 CMD: {}, Command: {}, Payload: {:?}",
                        format!("{:.2?}", elapsed),
                        times,
                        format!("{:.2?}", avg_elapsed),
                        action,
                        command,
                        decoded_payload
                    );
                    // 顯示接收到的 CMD 和 Command
                    self.messages.push(message.clone());
                    println!("{}", message);
                    if *times % 100 == 0 {
                        self.send_motor(Action::SEND, Command::MOTOR).await?;
                    }
                }
                *times += 1; // 增加次數
                *start_time = std::time::Instant::now(); // 重置計時器
                self.idx += 1;
                return Ok(0);
            }
            _ => {
                self.reset().await;
                return Ok(0);
                // return Err(anyhow::anyhow!("Unexpected index: {}", self.idx));
                // continue;
            }
        }
    }

    pub async fn listen(&mut self) -> Result<(), Error> {
        let mut action = Action::NONE; // 初始 Action
        let mut command = Command::NONE; // 初始 Command
        let mut times = 0; // 用於計算平均耗時
        let mut start_time = std::time::Instant::now();
        let mut elapsed_list = Vec::<Duration>::new(); // 用於存儲每次的耗時
        loop {
            let mut buf = [0u8; 1];
            let read_result = self.port.read(&mut buf);
            if read_result.is_err() {
                self.messages.push("讀取串口資料失敗，可能是串口已關閉或發生錯誤".into());
                // 嘗試關閉並重新打開串口
                self.messages.push(format!("關閉序列埠: {}", self.port_name));
                sleep(Duration::from_secs(1)); // 等待 1 秒鐘
                // 嘗試重新打開串口
                self.port = match
                    open_serial_port(
                        &self.port_name,
                        self.baud_rate,
                        self.timeout,
                        self.max_retries
                    )
                {
                    Ok(p) => p,
                    Err(e) => {
                        eprintln!("無法重新打開序列埠 {}: {}", self.port_name, e);
                        return Err(anyhow::anyhow!("無法重新打開序列埠"));
                    }
                };
                self.messages.push(format!("重新打開序列埠: {}", self.port_name));
                sleep(Duration::from_secs(1)); // 等待 1 秒鐘
                continue; // 重新開始循環
            } else if read_result.is_ok() {
                self.buffer[self.idx] = buf[0];
                println!("Buffer: {:02X?}, idx: {}", &self.buffer[..self.idx + 1], self.idx);
                // if
                //     let Ok(ret) = self.process_frame(
                //         &mut times,
                //         &mut action,
                //         &mut command,
                //         &mut start_time,
                //         &mut elapsed_list
                //     ).await
                // {
                //     if ret != 0 {
                //         break;
                //     }
                //     continue;
                // }
                self.idx += 1;
                if self.idx >= MAX_DATA_LEN {
                    self.messages.push("Buffer overflow, resetting...".into());
                    self.reset().await;
                    continue;
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
            eprintln!("Frame too short: expected at least 6 bytes, got {}", frame_size);
            return Err(
                anyhow::anyhow!("Frame too short: expected at least 6 bytes, got {}", frame_size)
            );
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
        let payload = serde_cbor
            ::from_slice::<HashMap<String, Value>>(payload_bytes)
            .map_err(|e| anyhow::anyhow!("CBOR decode error: {}", e))?;
        // 2 bytes, CRC
        let crc_bytes = &frame[frame_size - 2..frame_size];
        let crc = u16::from_le_bytes(crc_bytes.try_into().unwrap());
        let calc_crc = CRC16.checksum(&frame[2..frame_size - 2]);
        if crc != calc_crc {
            eprintln!("CRC mismatch: expected {:04X}, got {:04X}", calc_crc, crc);
            return Err(anyhow::anyhow!("CRC mismatch: expected {:04X}, got {:04X}", calc_crc, crc));
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
        self.messages.push(format!("傳送資料: {:02X?}", frame));
        self.port.write_all(frame)?;
        self.port.flush()?;
        self.messages.push("資料傳送成功".into());
        Ok(())
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // payload
    let payload = StateMessage { status: 0 };
    println!("{:30} {}, size: {}", "PayLoad:", payload, std::mem::size_of_val(&payload));

    // 將 payload 序列化為 CBOR 格式
    let payload_cbor = serde_cbor::to_vec(&payload)?;
    println!("{:30} {:02X?}, size: {}", "PayLoad CBOR:", payload_cbor, payload_cbor.len());

    // 建立要傳送的 frame
    let (frame, _crc) = GigaCommunicate::build_frame(
        Action::READ,
        Command::SensorHIGH,
        &payload_cbor
    );
    println!(
        "{:30} {:02X?}, len: {}, {:02X?}, CRC: {:02X?}",
        "Send CBOR without COBS Frame:",
        frame,
        frame.len(),
        (frame.len() as u16).to_le_bytes(),
        _crc.to_le_bytes()
    );

    // 建立 COBS 編碼的 frame
    let (cobs_frame, _cobs_size, crc) = GigaCommunicate::build_cobs_frame(
        Action::SEND,
        Command::MOTOR,
        &payload_cbor
    );
    println!(
        "{:30} {:02X?}, size: {}, {:02X?}, CRC: {:02X?}",
        "CBOR with COBS Frame:",
        cobs_frame,
        _cobs_size,
        (_cobs_size as u16).to_le_bytes(),
        crc.to_le_bytes()
    );

    // 將 COBS 編碼的 frame 包裝成完整的傳送 frame
    // 這裡假設 START_BYTE 為 0x00，實際應根據協議定義
    let send_frame = vec![0x00].into_iter().chain(cobs_frame.clone()).collect::<Vec<u8>>();
    let send_frame_size = send_frame.len();
    println!("{:30} {:02X?}, size: {}", "Send CBOR with COBS Frame:", send_frame, send_frame_size);

    // 模擬 COBS 解碼
    let mut decoded_frame = vec![0; _cobs_size - 1]; // COBS 解碼後長度會減少
    let decoded_report = decode(&cobs_frame, &mut decoded_frame)?;
    println!(
        "{:30} {:02X?}, size: {}",
        "Decoded COBS Frame:",
        decoded_frame,
        decoded_report.frame_size()
    );

    let decoded_message = GigaCommunicate::decode_message(&decoded_frame)?;
    println!(
        "{:30} Action: {:?}, Command: {:?}, Payload Size bytes: {:02X?}, CRC bytes: {:02X?}",
        "Decoded Message:",
        decoded_message.action,
        decoded_message.command,
        decoded_message.payload_size_bytes,
        decoded_message.crc_bytes
    );
    println!(
        "{:30} {:02X?}, size: {}",
        "Decoded Payload Bytes:",
        decoded_message.payload_bytes,
        decoded_message.payload_size
    );
    println!("{:30} {:?}", "Decoded Payload:", decoded_message.payload);

    let args: Vec<String> = std::env::args().collect();
    let port_name = if args.len() > 1 {
        &args[1]
    } else {
        "COM5" // 默認端口
    };
    println!("使用串口: {}", port_name);
    // let frame = build_frame(CMD::SEND, Command::MOTOR, &payload_cbor);
    // let dst_frame = frame.clone();
    let timeout = Duration::from_secs(1);
    // 2️⃣ 打開序列埠
    for port in serialport::available_ports()? {
        println!("Found port: {}", port.port_name);
    }
    let max_retries = 5; // 最大重試次數
    let mut giga = GigaCommunicate::new(port_name, BAUD, timeout, max_retries)?;

    println!("成功打開序列埠: {}", port_name);
    // println!("等待 1 秒鐘...");
    // sleep(Duration::from_secs(1));

    // 4️⃣ 等待回覆
    println!("等待回覆...");
    giga.listen().await?;
    // for message in &giga.messages {
    //     println!("{}", message);
    // }
    // println!("Buffer: {:02X?}", &giga.buffer[..giga.idx + 1]);
    Ok(())
}
