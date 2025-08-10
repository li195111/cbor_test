use std::{ collections::HashMap, fmt::Display };
use serde::{ Serialize, Deserialize };
use serde_cbor::Value;

#[derive(Debug, Clone, Copy)]
pub enum ReceiveState {
    Normal,
    Debug,
    CheckingDebug(usize), // 參數表示已匹配的 DEBUG 字符數量
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Action {
    /// 發送資料
    SEND = 0xaa,

    /// 讀取資料
    READ = 0xa8,

    /// GIGA Notification
    GIGA = 0xae,

    /// 無效指令
    NONE = 0x00,
}

impl TryFrom<u8> for Action {
    type Error = &'static str;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0xaa => Ok(Action::SEND),
            0xa8 => Ok(Action::READ),
            0xae => Ok(Action::GIGA),
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
    /// 無效指令
    NONE = 0x00,

    /// 確認收到
    ACK = 0x01,

    /// 未確認收到
    NACK = 0x02,

    /// 馬達控制
    MOTOR = 0x03,

    /// 設定 ID
    SetID = 0x04,

    /// 檔案傳輸
    FILE = 0x05,

    /// 高位元感測器
    Sensor = 0x06,

    /// 低位元感測器
    SensorLOW = 0x07,
}

impl From<u8> for Command {
    fn from(value: u8) -> Self {
        match value {
            0x00 => Command::NONE,
            0x01 => Command::ACK,
            0x02 => Command::NACK,
            0x03 => Command::MOTOR,
            0x04 => Command::SetID,
            0x05 => Command::FILE,
            0x06 => Command::Sensor,
            0x07 => Command::SensorLOW,
            _ => Command::NONE,
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
    /// 馬達名子, string "PMt" / "PMb"
    pub name: String,
    /// ID, 數值 int8, 1~254, Read/Write
    pub id: u8,
    /// 馬達動作, 數值int8, 0: 停止, 1:轉動, Read/Write
    pub motion: u8,
    /// 設定的速度, 數值 int64, 有正負, -15000~15000, Read/Write
    pub speed: i64,
    /// %誤差範圍, 數值int8, 0~100
    pub tol: u8,
    /// 距離, 數值 int64
    pub dist: u64,
    /// 轉動角度, 數值 int64, 0~359
    pub angle: u64,
    /// 轉動時間, 數值 int64, ms
    pub time: u64,
    /// 加速度, 數值 int64, 0~66635, Read/Write
    pub acc: u64,
    /// 改變後新id,  數值 int8, 1~254, Read/Write
    pub newid: u8,
    /// 電壓 float, 0.0~27.0, Read Only
    pub volt: f32,
    /// 電流 float, 0.0~6.0, Read Only
    pub amp: f32,
    /// 溫度 float, -40.0~125.0, Read Only
    pub temp: f32,
    /// 馬達運行模式, 數值int8, 0:default, 1:位置, 2:速度
    pub mode: u8,
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
        match serde_json::to_string_pretty(&self.payload) {
            Ok(json) => write!(f, "PayloadMessage: {}", json),
            Err(_) => write!(f, "Error converting payload to JSON"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    /// 動作
    pub action: Action,
    /// 指令
    pub command: Command,
    /// Payload 大小 bytes
    pub payload_size_bytes: Vec<u8>,
    /// Payload 大小
    pub payload_size: u16,
    /// Payload 資料 bytes
    pub payload_bytes: Vec<u8>,
    /// Payload 資料
    pub payload: HashMap<String, Value>,
    /// CRC 校驗碼 bytes
    pub crc_bytes: Vec<u8>,
    /// CRC 校驗碼
    pub crc: u16,
}

#[allow(dead_code)]
impl Message {
    pub fn new(
        action: Action,
        command: Command,
        payload_size_bytes: Vec<u8>,
        payload_size: u16,
        payload_bytes: Vec<u8>,
        payload: HashMap<String, Value>,
        crc_bytes: Vec<u8>,
        crc: u16
    ) -> Self {
        Self {
            action,
            command,
            payload_size_bytes,
            payload_size,
            payload_bytes,
            payload,
            crc_bytes,
            crc,
        }
    }
}

impl Default for Message {
    fn default() -> Self {
        Self {
            action: Action::NONE,
            command: Command::NONE,
            payload_size_bytes: Vec::new(),
            payload_size: 0,
            payload_bytes: Vec::new(),
            payload: HashMap::new(),
            crc_bytes: Vec::new(),
            crc: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
pub enum ErrorCode {
    Success = 0,
    DataMissMatch = 1001,
    UnknownError,
    MemoryOverload,
    DecodeCBORError,
    CRCError,
    DataOverload,
    StorageAccessFailure,
    FileAccessViolation,
    WiFiPartitionError,
    UserPartitionError,
    MountingFileSystemFailed,
    InvalidID,
    InvalidCMD,
    SendCMDFail,
    WriteNVRamFail,
}

impl From<u32> for ErrorCode {
    fn from(value: u32) -> Self {
        match value {
            0 => ErrorCode::Success,
            1001 => ErrorCode::DataMissMatch,
            1002 => ErrorCode::UnknownError,
            1003 => ErrorCode::MemoryOverload,
            1004 => ErrorCode::DecodeCBORError,
            1005 => ErrorCode::CRCError,
            1006 => ErrorCode::DataOverload,
            1007 => ErrorCode::StorageAccessFailure,
            1008 => ErrorCode::FileAccessViolation,
            1009 => ErrorCode::WiFiPartitionError,
            1010 => ErrorCode::UserPartitionError,
            1011 => ErrorCode::MountingFileSystemFailed,
            1012 => ErrorCode::InvalidID,
            1013 => ErrorCode::InvalidCMD,
            1014 => ErrorCode::SendCMDFail,
            1015 => ErrorCode::WriteNVRamFail,
            _ => ErrorCode::UnknownError, // 預設為未知錯誤
        }
    }
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match serde_json::to_string_pretty(self) {
            Ok(json) => write!(f, "{}", json),
            Err(_) => write!(f, "Error converting to JSON"),
        }
    }
}
