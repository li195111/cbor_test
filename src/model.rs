use std::{ collections::HashMap, fmt::Display };
use serde::{ Serialize, Deserialize };
use serde_cbor::Value;

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum Action {
    SEND = 0xaa, // 發送資料
    READ = 0xa8, // 讀取資料
    GIGA = 0xae, // GIGA Notification
    NONE = 0x00, // 無效指令
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
    NONE = 0x00, // 無效指令
    ACK = 0x01, // 確認收到
    NACK = 0x02, // 未確認收到
    MOTOR = 0x03, // 馬達控制
    SetID = 0x04, // 設定 ID
    FILE = 0x05, // 檔案傳輸
    Sensor = 0x06, // 高位元感測器
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
            0x06 => Ok(Command::Sensor),
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
        write!(f, "PayloadMessage(payload: {:?})", self.payload)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub action: Action, // 動作
    pub command: Command, // 指令
    pub payload_size_bytes: Vec<u8>, // Payload 大小 bytes
    pub payload_size: u16, // Payload 大小
    pub payload_bytes: Vec<u8>, // Payload 資料 bytes
    pub payload: HashMap<String, Value>, // Payload 資料
    pub crc_bytes: Vec<u8>, // CRC 校驗碼 bytes
    pub crc: u16, // CRC 校驗碼
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
        match self {
            ErrorCode::Success => write!(f, "Success"),
            ErrorCode::DataMissMatch => write!(f, "Data Miss Match"),
            ErrorCode::UnknownError => write!(f, "Unknown Error"),
            ErrorCode::MemoryOverload => write!(f, "Memory Overload"),
            ErrorCode::DecodeCBORError => write!(f, "Decode CBOR Error"),
            ErrorCode::CRCError => write!(f, "CRC Error"),
            ErrorCode::DataOverload => write!(f, "Data Overload"),
            ErrorCode::StorageAccessFailure => write!(f, "Storage Access Failure"),
            ErrorCode::FileAccessViolation => write!(f, "File Access Violation"),
            ErrorCode::WiFiPartitionError => write!(f, "WiFi Partition Error"),
            ErrorCode::UserPartitionError => write!(f, "User Partition Error"),
            ErrorCode::MountingFileSystemFailed => write!(f, "Mounting File System Failed"),
            ErrorCode::InvalidID => write!(f, "Invalid ID"),
            ErrorCode::InvalidCMD => write!(f, "Invalid Command"),
            ErrorCode::SendCMDFail => write!(f, "Send Command Fail"),
            ErrorCode::WriteNVRamFail => write!(f, "Write NVRAM Fail"),
        }
    }
}
