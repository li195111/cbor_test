use std::{thread::sleep, time::Duration};
use cobs::encode;
use serde::{ Serialize, Deserialize };
use crc::{ Crc, CRC_16_USB };

const PORT: &str = "COM5";
const BAUD: u32 = 115_200;
const CRC16: Crc<u16> = Crc::<u16>::new(&CRC_16_USB);

/// 對應 Arduino encode_cbor() 內兩個 9 元素子陣列
#[derive(Debug, Serialize, Deserialize)]
struct Motion {
    name: String, // 馬達名子 string "PMt" / "PMb"
    id: u8, // 數值 int8
    motion: u8, // 馬達動作，數值int8，0: 停止，1:轉動
    speed: i64, // 設定的速度，數值 int64，有正負
    tol: u8, // %誤差範圍，數值int8，0~100
    dist: u32, // 距離，數值 int64
    angle: u32, // 轉動角度，數值int64，0~359
    time: u32, // 轉動時間，數值int64, ms
    acc: u32, // 加速度，數值 int64
    newid: u8, // 改變後新id，數值 int8
    volt: f32, // 電壓， float
    amp: f32, // 電流， float
    temp: f32, // 溫度， float
    mode: u8, // 馬達運行模式，數值int8，0:default，1:位置，2:速度
}

fn main() -> anyhow::Result<()> {
    // 1️⃣ 準備資料
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
    let payload = vec![m1, m2]; // 對應外層 3 元素陣列
    // println!("Payload: {:#?}", payload);
    let payload_cbor: Vec<u8> = serde_cbor::to_vec(&payload)?;
    // for i in 0..payload_cbor.len() {
    //     if i % 16 == 0 {
    //         println!();
    //     }
    //     print!("{:02x} ", payload_cbor[i]);
    // }
    println!("\nCBOR 長度: {}", payload_cbor.len());
    let cmd_byte = 0x01u8;
    let mut frame = vec![0, 0]; // 前兩個 byte 預留給長度
    frame.push(cmd_byte); // 指令 byte
    frame.extend(&payload_cbor); // 加入 CBOR 資料
    let crc = CRC16.checksum(&frame[2..]); // 計算 CRC 從第 3 個 byte 開始, 指令+CBOR 資料
    println!("Rust 端 CRC (hex): {:04X}", crc);
    frame.extend(crc.to_be_bytes());
    let len = (frame.len() - 2) as u16; // 長度為總長度減去前兩個 byte
    frame[0] = (len >> 8) as u8; // 高位
    frame[1] = len as u8; // 低位

    let mut dst_frame = vec![0; frame.len() + 1]; // COBS 編碼後長度會增加
    let _encoded_size = encode(&frame, &mut dst_frame);
    // println!("Encoded Frame size: {}", encoded_size);
    // println!("Encoded Frame: {:?}", &dst_frame);
    for i in 0..dst_frame.len() {
        if i % 16 == 0 {
            println!();
        }
        print!("{:02x} ", dst_frame[i]);
    }
    // 2️⃣ 打開序列埠
    let mut port = serialport
        ::new(PORT, BAUD)
        .timeout(Duration::from_millis(500))
        .open()
        .expect("Failed to open serial port");

    port.write_all(b"SEND\n")?;
    port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
    port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
    port.flush()?; // 確保資料已經寫入

    // 4️⃣ 等待回覆
    let start_time = std::time::Instant::now();
    let mut line = String::new();
    let mut times = 1;
    loop {
        let mut buf = [0u8; 1];
        if port.read(&mut buf).is_ok() {
            let ch = buf[0] as char;
            if ch == '\n' {
                match line.trim() {
                    "STATUS" => println!("{} ✓ Arduino 收到封包", line.trim()),
                    "good" => {
                        println!("{} ✓ Arduino 處理成功", line.trim());
                        let elapsed = start_time.elapsed();
                        println!("總共花費時間: {:.2?}, 次數: {} 平均: {:.2?}", elapsed, times, elapsed / times);
                        sleep(Duration::from_millis(1));
                        port.write_all(b"SEND\n")?;
                        port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
                        port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
                        port.flush()?; // 確保資料已經寫入
                        times += 1;
                        if times > 100 {
                            break;
                        }
                    }
                    "starting..." => {
                        port.write_all(b"SEND\n")?;
                        port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
                        port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
                        port.flush()?; // 確保資料已經寫入
                        times += 1;
                    }
                    "Arduino CBOR Receiver Ready" => {
                        port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
                        port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
                        port.flush()?; // 確保資料已經寫入
                        times += 1;
                    }
                    "[OK ]" => {
                        println!("{} ✓ Arduino 回應 OK", line.trim());
                        let elapsed = start_time.elapsed();
                        println!("總共花費時間: {:.2?}, 次數: {}", elapsed, times);
                        break;
                    }
                    other => {
                        println!("ℹ️  其他訊息: {other}");
                    }
                }
                line.clear();
            } else {
                line.push(ch);
            }
        }

        // port.write_all(b"SEND\n")?;
        // port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
        // port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
        // port.flush()?; // 確保資料已經寫入
        // times += 1;
    }

    Ok(())
}
