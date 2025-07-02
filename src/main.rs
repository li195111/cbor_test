use std::{ thread::sleep, time::Duration };
use cobs::encode;
use serde::{ Serialize, Deserialize };
use crc::{ Crc, CRC_16_USB };

const BAUD: u32 = 460_800;
const CRC16: Crc<u16> = Crc::<u16>::new(&CRC_16_USB);
const START_BYTE: u8 = 0x7E; // 開始 byte

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

fn build_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 5);
    
    frame.push(START_BYTE); // 開始 byte

    let len = payload.len() as u16;
    frame.extend(len.to_le_bytes());
    println!("Payload 長度: {} {:02X?}", len, len.to_le_bytes());

    frame.extend(payload);

    let crc = CRC16.checksum(&frame[1..]); // 跳過 START_BYTE
    frame.extend(crc.to_le_bytes()); // lo, hi
    println!("CRC: {} {:02X?}", crc, crc.to_le_bytes());

    frame
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let port_name = if args.len() > 1 {
        &args[1]
    } else {
        "COM5" // 默認端口
    };
    println!("使用串口: {}", port_name);
    // 1️⃣ 準備資料
    let m1 = Motion {name: "PMt".into(),id: 5,motion: 1,speed: 100,tol: 5,dist: 2000,angle: 100,time: 5000,acc: 300,newid: 0,volt: 12.0,amp: 0.5,temp: 25.0,mode: 0};
    let m2 = Motion {name: "PMb".into(),id: 4,motion: 1,speed: 100,tol: 2,dist: 1900,angle: 60,time: 4000,acc: 400,newid: 0,volt: 12.0,amp: 0.6,temp: 26.0,mode: 0};
    let payload = vec![m1, m2]; // 對應外層 3 元素陣列
    println!("Payload: {:?}", payload);
    let payload_cbor: Vec<u8> = serde_cbor::to_vec(&payload)?;
    println!("CBOR 資料: {:02X?}", payload_cbor);
    println!("CBOR 長度: {}", payload_cbor.len());
    let frame = build_frame(&payload_cbor);
    let dst_frame = frame.clone();
    // let mut dst_frame = vec![0; frame.len() + 1]; // COBS 編碼後長度會增加
    // let _encoded_size = encode(&frame, &mut dst_frame);
    // // println!("Encoded Frame size: {}", encoded_size);
    // println!("Encoded Frame: {:02X?}", dst_frame);
    // 2️⃣ 打開序列埠
    let mut port = serialport
        ::new(port_name, BAUD)
        .timeout(Duration::from_millis(500))
        .open()
        .expect("Failed to open serial port");

    // port.write_all(b"SEND\n")?;
    port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
    // port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
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
                        println!(
                            "總共花費時間: {:.2?}, 次數: {} 平均: {:.2?}",
                            elapsed,
                            times,
                            elapsed / times
                        );
                        sleep(Duration::from_millis(1));
                        // port.write_all(b"SEND\n")?;
                        port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
                        // port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
                        port.flush()?; // 確保資料已經寫入
                        times += 1;
                        if times > 1 {
                            break;
                        }
                    }
                    "starting..." => {
                        println!("{} ✓ Arduino 開始處理", line.trim());
                        // port.write_all(b"SEND\n")?;
                        port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
                        // port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
                        port.flush()?; // 確保資料已經寫入
                        times += 1;
                    }
                    "Arduino CBOR Receiver Ready" => {
                        println!("{} ✓ Arduino 已準備好接收 CBOR 資料", line.trim());
                        port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
                        // port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
                        port.flush()?; // 確保資料已經寫入
                        times += 1;
                    }
                    "CBOR Motor Receiver Ready" => {
                        println!("{} ✓ Arduino 已準備好接收馬達資料", line.trim());
                        port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
                        // port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
                        port.flush()?; // 確保資料已經寫入
                        times += 1;
                    }
                    "Ready for next frame" => {
                        let elapsed = start_time.elapsed();
                        println!("{} ✓ Arduino 準備好接收下一個幀, 平均耗時: {:.2?}, 次數: {}", line.trim(), elapsed / times, times);
                        port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
                        // port.write_all(&[0x00])?; // COBS 編碼後的結尾 byte
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
