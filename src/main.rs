mod model;

use std::{ collections::HashMap, thread::sleep, time::Duration, vec };
#[allow(unused_imports)]
use cobs::encode;
use crc::{ Crc, CRC_16_USB };
use serde_cbor::Value;
use model::{ CMD, Command, Motion };

const BAUD: u32 = 460_800;
const CRC16: Crc<u16> = Crc::<u16>::new(&CRC_16_USB);
const START_BYTE: u8 = 0x7e; // 開始 byte
const MAX_DATA_LEN: usize = 1024;

fn build_frame(cmd: CMD, command: Command, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 5);

    frame.push(START_BYTE); // 開始 byte
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

fn main() -> anyhow::Result<()> {
    let payload = 0; // 空的 payload
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
    let payload: Vec<Motion> = vec![m1, m2]; // 對應外層 3 元素陣列
    println!("Payload: {:?}", payload);
    let payload_cbor: Vec<u8> = serde_cbor::to_vec(&payload)?;
    println!("CBOR 資料: {:02X?}", payload_cbor);
    println!("CBOR 長度: {}", payload_cbor.len());
    let frame = build_frame(CMD::SEND, Command::MOTOR, &payload_cbor);
    let dst_frame = frame.clone();
    // let mut dst_frame = vec![0; frame.len() + 1]; // COBS 編碼後長度會增加
    // let _encoded_size = encode(&frame, &mut dst_frame);
    // // println!("Encoded Frame size: {}", encoded_size);
    println!("Encoded Frame: {:02X?}", dst_frame);
    let timeout = Duration::from_secs(1);
    // 2️⃣ 打開序列埠
    for port in serialport::available_ports()? {
        println!("Found port: {}", port.port_name);
    }
    let port_result = serialport::new(port_name, BAUD).timeout(timeout).open();
    let mut port = match port_result {
        Ok(p) => p,
        Err(e) => {
            eprintln!("無法打開序列埠 {}: {}", port_name, e);
            return Err(anyhow::anyhow!("無法打開序列埠"));
        }
    };
    println!("成功打開序列埠: {}", port_name);
    println!("等待 1 秒鐘...");
    sleep(Duration::from_secs(1));
    // 3️⃣ 傳送資料
    println!("傳送資料: {:02X?}", dst_frame);
    port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
    port.flush()?; // 確保資料已經寫入

    // 4️⃣ 等待回覆
    println!("等待回覆...");
    let start_time = std::time::Instant::now();

    let mut times = 1;
    let mut idx = 0; // 用於追蹤 START_BYTE 的索引位置
    let mut buffer = Vec::<u8>::with_capacity(MAX_DATA_LEN); // 用於存儲接收的資料

    let mut cmd = CMD::SEND; // 初始 CMD
    let mut command = Command::ACK; // 初始 Command
    let mut len_bytes = [0u8; 2]; // 用於存儲長度的 byte
    let mut payload_size = 0 as usize;
    let mut payload_data = Vec::<u8>::new(); // 用於存儲 payload 部分
    let mut crc_bytes = [0u8; 2]; // 用於存儲 CRC 的 byte

    let max_retries = 5; // 最大重試次數
    let mut retries = 0; // 當前重試次數
    loop {
        let mut buf = [0u8; 1];
        let read_result = port.read(&mut buf);
        if read_result.is_err() {
            println!("讀取串口資料失敗，可能是串口已關閉或發生錯誤");
            // 嘗試關閉並重新打開串口
            drop(port);
            println!("關閉序列埠: {}", port_name);
            sleep(Duration::from_secs(1)); // 等待 1 秒鐘
            // 嘗試重新打開串口
            loop {
                match serialport::new(port_name, BAUD).timeout(timeout).open() {
                    Ok(p) => {
                        port = p;
                        println!("成功重新打開序列埠: {}", port_name);
                        break;
                    }
                    Err(e) => {
                        retries += 1;
                        if retries >= max_retries {
                            eprintln!("無法重新打開序列埠 {}: {}", port_name, e);
                            return Err(anyhow::anyhow!("無法重新打開序列埠"));
                        }
                        eprintln!(
                            "無法重新打開序列埠 {}: {}, 正在重試第 {} 次",
                            port_name,
                            e,
                            retries
                        );
                        sleep(Duration::from_secs(1));
                        continue;
                    }
                }
            }
            println!("重新打開序列埠: {}", port_name);
            sleep(Duration::from_secs(1)); // 等待 1 秒鐘
            continue; // 重新開始循環
        }
        if read_result.is_ok() {
            let ch = buf[0] as u8;
            // println!("接收到 byte: {:02X?}", ch);
            // sleep(Duration::from_millis(100));
            match ch {
                START_BYTE => {
                    // println!("idx: {}, 接收到 byte: {:02X?}", idx, ch);
                    idx = 0; // 重置索引
                    buffer.clear(); // 清空 buffer
                    len_bytes = [0u8; 2]; // 重置長度 byte
                    crc_bytes = [0u8; 2]; // 重置 CRC byte
                    continue; // 跳過後續處理
                }
                val if idx == 0 && CMD::try_from(val).is_ok() => {
                    idx += 1; // 確認收到 CMD
                    buffer.push(ch);
                    cmd = CMD::try_from(val).unwrap();
                    // println!("idx: {}, 收到 CMD: {:02X?}", idx, cmd);
                    continue; // 跳過後續處理
                }
                val if idx == 1 && Command::try_from(val).is_ok() => {
                    idx += 1; // 確認收到 Command
                    buffer.push(ch);
                    command = Command::try_from(val).unwrap();
                    // println!("idx: {}, 收到 CMD: {:02X?}", idx, command);
                    continue; // 跳過後續處理
                }
                _val if idx == 2 => {
                    idx += 1; // 確認收到長度 byte
                    buffer.push(ch);
                    len_bytes[0] = ch; // 第一個長度 byte
                    continue; // 跳過後續處理
                }
                _val if idx == 3 => {
                    idx += 1; // 確認收到第二個長度 byte
                    buffer.push(ch);
                    len_bytes[1] = ch; // 第二個長度 byte
                    payload_size = u16::from_le_bytes(
                        len_bytes.as_slice().try_into().unwrap()
                    ) as usize;
                    if payload_size > MAX_DATA_LEN {
                        println!("Payload 長度超過最大限制: {}", MAX_DATA_LEN);
                        idx = 0; // 重置索引
                        buffer.clear(); // 清空 buffer
                        len_bytes = [0u8; 2]; // 重置長度 byte
                        crc_bytes = [0u8; 2]; // 重置 CRC byte
                        continue; // 跳過後續處理
                    }
                    // println!(
                    //     "idx: {}, 收到長度: {:02X?}, Payload 長度: {}",
                    //     idx,
                    //     len_bytes,
                    //     payload_size
                    // );
                    continue; // 跳過後續處理
                }
                _val if idx >= 4 && idx < 4 + payload_size => {
                    idx += 1; // 繼續接收 payload 部分
                    buffer.push(ch);
                    payload_data.push(ch);
                    // println!(
                    //     "idx: {}, 收到 payload: {:02X?}, ",
                    //     idx,
                    //     buffer[4..4 + payload_size].to_vec()
                    // );
                    continue; // 跳過後續處理
                }
                _val if idx == 4 + payload_size => {
                    idx += 1; // 確認收到第一個 CRC byte
                    buffer.push(ch);
                    crc_bytes[0] = ch; // 第一個 CRC byte
                    continue; // 跳過後續處理
                }
                _val if idx == 4 + payload_size + 1 => {
                    idx += 1; // 確認收到第二個 CRC byte
                    buffer.push(ch);
                    crc_bytes[1] = ch; // 第二個 CRC byte
                    // println!("idx: {}, 收到 CRC bytes: {:02X?}", idx, crc_bytes);
                    let crc = u16::from_le_bytes(crc_bytes.as_slice().try_into().unwrap());
                    // println!("接收到的 CRC: {:04X}", crc);
                    let calc_crc = CRC16.checksum(&buffer[2..4 + payload_size]); // 跳過 START_BYTE, Cmd Byte, Command Byte
                    // println!("計算的 CRC: {:04X}", calc_crc);
                    if crc == calc_crc {
                        // println!("CRC 驗證成功");
                        // 將 payload 部分轉換為 CBOR 資料
                        // let mut result = None;
                        if
                            payload_size == 1 &&
                            (command == Command::SensorHIGH || command == Command::SensorLOW)
                        {
                            // payload 不重要
                            // result = Some(format!("Sensor Command: {}", command));
                            let elapsed = start_time.elapsed();
                            println!(
                                "平均耗時: {:.2?}, 次數: {}, 接收到的資料: {}",
                                elapsed / times,
                                times,
                                format!("Sensor cmd: {}, Command: {}", cmd, command)
                            );
                            port.write_all(&dst_frame)?; // 傳送 COBS 編碼後的資料
                            port.flush()?; // 確保資料已經寫入
                        } else {
                            let decoded_payload: HashMap<String, Value> = serde_cbor::from_slice(
                                &buffer[4..4 + payload_size]
                            )?;
                            // result = Some(
                            //     format!("Payload: {:?}, Command: {}", decoded_payload, command)
                            // );
                            let elapsed = start_time.elapsed();
                            println!(
                                "平均耗時: {:.2?}, 次數: {}, 接收到的資料: {}",
                                elapsed / times,
                                times,
                                format!(
                                    "Payload: {:?}, cmd: {}, Command: {}",
                                    decoded_payload,
                                    cmd,
                                    command
                                )
                            );
                        }
                        // let elapsed = start_time.elapsed();
                        // println!(
                        //     "平均耗時: {:.2?}, 次數: {}, 接收到的資料: {}",
                        //     elapsed / times,
                        //     times,
                        //     result.clone().unwrap()
                        // );
                        // if !result.unwrap().contains("Sensor Command") {
                        //     break;
                        // }
                        times += 1;
                        continue; // 跳過後續處理
                    } else {
                        // println!("CRC 驗證失敗");
                        idx = 0; // 重置索引
                        buffer.clear(); // 清空 buffer
                        len_bytes = [0u8; 2]; // 重置長度 byte
                        crc_bytes = [0u8; 2]; // 重置 CRC byte
                        continue; // 跳過後續處理
                    }
                }
                _ => {
                    idx += 1; // 增加索引
                    buffer.push(ch); // 將接收到的 byte 添加到 buffer
                    continue; // 跳過後續處理
                }
            }
        }
    }

    Ok(())
}
