mod model;

use std::{ collections::HashMap, fmt::Display, thread::sleep, time::Duration, vec };
use anyhow::Error;
#[allow(unused_imports)]
use cobs::encode;
use crc::{ Crc, CRC_16_USB };
use serde::{ Serialize, Deserialize };
use serde_cbor::Value;
use model::{ CMD, Command, Motion };

const BAUD: u32 = 460_800;
const CRC16: Crc<u16> = Crc::<u16>::new(&CRC_16_USB);
const START_BYTE: [u8; 1] = [0x7e]; // 開始 byte
const MAX_DATA_LEN: usize = 1024;

fn build_frame(cmd: CMD, command: Command, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 5);

    frame.extend(&START_BYTE); // 開始 byte
    // Cmd Byte, SEND: 0xAA, READ: 0xA8
    frame.push(cmd as u8);
    println!("Cmd Byte: {:02X?}", frame[1]);

    // Command Byte, Ack=0x01, Nack=0x02, Motor=0x03, SetID=0x04, File=0x05, Sensor High=0x06, Sensor Low=0x07
    frame.push(command as u8);
    println!("Command Byte: {:02X?}", frame[2]);

    let len = payload.len() as u16;
    frame.extend(len.to_le_bytes());
    println!("Payload 長度: {} {:02X?}", len, len.to_le_bytes());

    frame.extend(payload);

    let crc = CRC16.checksum(&frame[3..]); // 跳過 START_BYTE, Cmd Byte, Command Byte
    frame.extend(crc.to_le_bytes()); // lo, hi
    println!("CRC: {} {:02X?}", crc, crc.to_le_bytes());

    frame
}

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

#[derive(Debug, Serialize, Deserialize)]
struct StateMessage {
    status: u8,
}

impl Display for StateMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "StateMessage(status: {})", self.status)
    }
}

#[allow(dead_code)]
struct PayloadMessage {
    payload: HashMap<String, Value>,
}

impl Display for PayloadMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "PayloadMessage(payload: {:?})", self.payload)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct GigaMessage {
    cmd: CMD,
    command: Command,
    message: Option<HashMap<String, Value>>,
}

#[allow(dead_code)]
impl GigaMessage {
    pub fn new(cmd: CMD, command: Command) -> Self {
        Self { cmd, command, message: None }
    }
}

impl Default for GigaMessage {
    fn default() -> Self {
        Self {
            cmd: CMD::NONE,
            command: Command::NONE,
            message: None,
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

    pub async fn send_motor(&mut self, cmd: CMD, command: Command) -> Result<(), Error> {
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
        let frame = self.build_frame(cmd, command, &payload_cbor);
        self.send(&frame)?;
        Ok(())
    }

    pub async fn listen(&mut self) -> Result<(), Error> {
        let mut cmd = CMD::NONE; // 初始 CMD
        let mut command = Command::NONE; // 初始 Command
        let mut times = 1; // 用於計算平均耗時
        let start_time = std::time::Instant::now();
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
                // println!("byte[{}]: {:02X}", self.idx, self.buffer[self.idx]);
                match self.idx {
                    0 => {
                        if self.buffer[self.idx] != START_BYTE[0] {
                            self.reset().await;
                            continue;
                        }
                        self.messages.push(
                            format!("收到 Start Byte: {:02X?}", self.buffer[self.idx])
                        );
                        self.idx += 1;
                    }
                    1 => {
                        if let Ok(c) = CMD::try_from(self.buffer[self.idx]) {
                            cmd = c;
                            self.messages.push(format!("收到 Cmd Byte: {:02X?}", cmd as u8));
                            self.idx += 1;
                        } else {
                            self.reset().await;
                            continue;
                        }
                    }
                    2 => {
                        if let Ok(c) = Command::try_from(self.buffer[self.idx]) {
                            command = c;
                            self.messages.push(
                                format!("收到 Command Byte: {:02X?}", command as u8)
                            );
                            self.idx += 1;
                            continue;
                        } else {
                            self.reset().await;
                            continue;
                        }
                    }
                    3 => {
                        self.len_bytes[0] = self.buffer[self.idx];
                        self.idx += 1;
                    }
                    4 => {
                        self.len_bytes[1] = self.buffer[self.idx];
                        self.payload_size = u16::from_le_bytes(self.len_bytes) as usize;
                        if self.payload_size > MAX_DATA_LEN - 7 {
                            return Err(anyhow::anyhow!("Payload size exceeds maximum limit"));
                        }
                        self.messages.push(format!("Payload Size: {} bytes", self.payload_size));
                        self.idx += 1;
                    }
                    idx if idx >= 5 && idx < 5 + self.payload_size => {
                        // 收到 payload 資料
                        self.idx += 1;
                    }
                    idx if idx >= 5 + self.payload_size && idx < 7 + self.payload_size => {
                        // 收到 CRC 資料
                        self.crc_bytes[self.idx - (5 + self.payload_size)] = self.buffer[self.idx];
                        if self.idx == 6 + self.payload_size {
                            let crc = u16::from_le_bytes(self.crc_bytes);
                            let calculated_crc = CRC16.checksum(
                                &self.buffer[3..5 + self.payload_size]
                            );
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
                            self.messages.push(
                                format!(
                                    "平均耗時: {:.2?}, 次數: {}, 接收到 CMD: {}, Command: {}, Payload: {:?}",
                                    elapsed / times,
                                    times,
                                    cmd,
                                    command,
                                    decoded_payload
                                )
                            );
                            println!(
                                "平均耗時: {:.2?}, 次數: {}, 接收到 CMD: {}, Command: {:10}, Payload: {:?}",
                                elapsed / times,
                                times,
                                cmd,
                                format!("{:?}", command),
                                decoded_payload
                            );
                            if times % 100 == 0 {
                                self.send_motor(CMD::SEND, Command::MOTOR).await?;
                            }
                        }
                        times += 1; // 增加次數
                        self.idx += 1;
                    }
                    _ => {
                        // break;
                        self.reset().await;
                        continue;
                    }
                }
            }
        }
        Ok(())
    }

    pub fn build_frame(&mut self, cmd: CMD, command: Command, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(payload.len() + 7);
        // 開始 byte
        frame.extend(START_BYTE); // 1 byte
        // Cmd Byte, SEND: 0xAA, READ: 0xA8
        frame.push(cmd as u8); // 1 byte
        self.messages.push(format!("Cmd Byte: {:02X?}", frame[1]));
        // Command Byte, Ack=0x01, Nack=0x02, Motor=0x03, SetID=0x04, File=0x05, Sensor High=0x06, Sensor Low=0x07
        frame.push(command as u8); // 1 byte
        self.messages.push(format!("Command Byte: {:02X?}", frame[2]));
        let len = payload.len() as u16;
        frame.extend(len.to_le_bytes()); // 2 bytes
        self.messages.push(format!("Payload 長度: {} {:02X?}", len, len.to_le_bytes()));
        frame.extend(payload); // payload 長度可變
        // 跳過 START_BYTE, Cmd Byte, Command Byte
        let crc = CRC16.checksum(&frame[3..]);
        // lo, hi  // 2 bytes
        frame.extend(crc.to_le_bytes());
        self.messages.push(format!("CRC: {} {:02X?}", crc, crc.to_le_bytes()));
        frame
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
    let payload = StateMessage { status: 0 }; // 空的 payload
    println!("Sensor PayLoad: {}", payload);
    let payload_cbor = serde_cbor::to_vec(&payload)?;
    println!("Sensor PayLoad CBOR: {:02X?}", payload_cbor);
    let frame = build_frame(CMD::READ, Command::SensorHIGH, &payload_cbor);
    println!("Sensor Frame: {:02X?}, len: {}", frame, frame.len());

    let args: Vec<String> = std::env::args().collect();
    let port_name = if args.len() > 1 {
        &args[1]
    } else {
        "COM5" // 默認端口
    };
    println!("使用串口: {}", port_name);
    // let frame = build_frame(CMD::SEND, Command::MOTOR, &payload_cbor);
    // let dst_frame = frame.clone();
    // let mut dst_frame = vec![0; frame.len() + 1]; // COBS 編碼後長度會增加
    // let _encoded_size = encode(&frame, &mut dst_frame);
    // // println!("Encoded Frame size: {}", encoded_size);
    // println!("Encoded Frame: {:02X?}", dst_frame);
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
    for message in &giga.messages {
        println!("{}", message);
    }
    println!("Buffer: {:02X?}", &giga.buffer[..giga.idx + 1]);
    Ok(())
}
