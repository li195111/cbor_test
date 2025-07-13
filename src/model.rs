use std::{ collections::HashMap, fmt::Display };
use serde::{ Serialize, Deserialize };
use serde_cbor::Value;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Action {
    SEND = 0xaa, // 發送資料
    READ = 0xa8, // 讀取資料
    NONE = 0x00, // 無效指令
}

impl TryFrom<u8> for Action {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0xaa => Ok(Action::SEND),
            0xa8 => Ok(Action::READ),
            0x00 => Ok(Action::NONE),
            _ => Err("Invalid CMD Byte"),
        }
    }
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Command {
    NONE = 0x00, // 無效指令
    ACK = 0x01, // 確認收到
    NACK = 0x02, // 未確認收到
    MOTOR = 0x03, // 馬達控制
    SetID = 0x04, // 設定 ID
    FILE = 0x05, // 檔案傳輸
    SensorHIGH = 0x06, // 高位元感測器
    SensorLOW = 0x07, // 低位元感測器
}

impl TryFrom<u8> for Command {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Command::NONE),
            0x01 => Ok(Command::ACK),
            0x02 => Ok(Command::NACK),
            0x03 => Ok(Command::MOTOR),
            0x04 => Ok(Command::SetID),
            0x05 => Ok(Command::FILE),
            0x06 => Ok(Command::SensorHIGH),
            0x07 => Ok(Command::SensorLOW),
            _ => Err("Invalid Command Byte"),
        }
    }
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// 對應 Arduino encode_cbor() 內兩個 9 元素子陣列
#[derive(Debug, Serialize, Deserialize)]
pub struct Motion {
    pub name: String, // 馬達名子 string "PMt" / "PMb"
    pub id: u8, // 數值 int8
    pub motion: u8, // 馬達動作，數值int8，0: 停止，1:轉動
    pub speed: i64, // 設定的速度，數值 int64，有正負
    pub tol: u8, // %誤差範圍，數值int8，0~100
    pub dist: u32, // 距離，數值 int64
    pub angle: u32, // 轉動角度，數值int64，0~359
    pub time: u32, // 轉動時間，數值int64, ms
    pub acc: u32, // 加速度，數值 int64
    pub newid: u8, // 改變後新id，數值 int8
    pub volt: f32, // 電壓， float
    pub amp: f32, // 電流， float
    pub temp: f32, // 溫度， float
    pub mode: u8, // 馬達運行模式，數值int8，0:default，1:位置，2:速度
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StateMessage {
    pub status: u8,
}

impl Display for StateMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "StateMessage(status: {})", self.status)
    }
}

#[allow(dead_code)]
pub struct PayloadMessage {
    pub payload: HashMap<String, Value>,
}

impl Display for PayloadMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "PayloadMessage(payload: {:?})", self.payload)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GigaMessage {
    pub action: Action,
    pub command: Command,
    pub message: Option<HashMap<String, Value>>,
}

#[allow(dead_code)]
impl GigaMessage {
    pub fn new(action: Action, command: Command) -> Self {
        Self { action, command, message: None }
    }
}

impl Default for GigaMessage {
    fn default() -> Self {
        Self {
            action: Action::NONE,
            command: Command::NONE,
            message: None,
        }
    }
}
