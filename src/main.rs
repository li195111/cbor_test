use std::{ thread::sleep, time::Duration };
use serde::{ Serialize, Deserialize };
use serialport;

const PORT: &str = "COM5";
const BAUD: u32 = 115_200;

/// 對應 Arduino encode_cbor() 內兩個 9 元素子陣列
#[derive(Serialize, Deserialize)]
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
    let payload = ("M", vec![m1, m2]); // 對應外層 3 元素陣列
    let cbor: Vec<u8> = serde_cbor::to_vec(&payload)?;
    for i in 0..cbor.len() {
        if i % 16 == 0 {
            println!();
        }
        print!("{:02x} ", cbor[i]);
    }
    println!("\nCBOR 長度: {}", cbor.len());

    // 2️⃣ 打開序列埠
    let mut port = serialport
        ::new(PORT, BAUD)
        .timeout(Duration::from_millis(50))
        .open()
        .expect("Failed to open serial port");

    // println!("等待 1 秒讓 Arduino 啟動...");
    sleep(Duration::from_secs_f32(1.0));
    // 3️⃣ 傳送
    port.write_all(b"SEND\n")?; // Arduino 讀到這行才準備接收
    port.write_all(&cbor)?; // Raw CBOR
    port.write_all(b"\n")?; // Arduino 以 '\n' 當封包結束
    port.flush()?;

    let start_time = std::time::Instant::now();
    // 4️⃣ 等待回覆
    let mut line = String::new();
    let mut times = 1;
    loop {
        // 3️⃣ 傳送
        // writeln!(port, "SEND {}", cbor.len())?;  // e.g. "SEND 210\n"

        let mut buf = [0u8; 1];
        if port.read(&mut buf).is_ok() {
            let ch = buf[0] as char;
            if ch == '\n' {
                match line.trim() {
                    "STATUS" => println!("{} ✓ Arduino 收到封包", line.trim()),
                    "good" => {
                        println!("{} ✓ Arduino 處理成功", line.trim());
                        let elapsed = start_time.elapsed();
                        println!("總共花費時間: {:.2?}, 次數: {}", elapsed, times);
                        break;
                    }
                    "starting..." => {
                        // 3️⃣ 傳送
                        port.write_all(b"SEND\n")?; // Arduino 讀到這行才準備接收
                        port.write_all(&cbor)?; // Raw CBOR
                        port.write_all(b"\n")?; // Arduino 以 '\n' 當封包結束
                        port.flush()?;
                        times += 1;
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

        // port.write_all(b"SEND\n")?; // Arduino 讀到這行才準備接收
        // port.write_all(&cbor)?; // Raw CBOR
        // port.write_all(b"\n")?; // Arduino 以 '\n' 當封包結束
        // port.flush()?;
        // times += 1;
        // if times == 1 {
        //     println!("傳送第 {} 次", times);
        // }
    }

    Ok(())
}
