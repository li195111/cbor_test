

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
        if self.idx > 3 {
            println!("重設: idx: {}, buffer: {:02X?}", self.idx, &self.buffer[..self.idx + 1]);
            for message in &self.messages {
                println!("\t{}", message);
            }
        }
        self.buffer = [0u8; MAX_DATA_LEN];
        self.idx = 0;
        self.len_bytes = [0u8; 2];
        self.crc_bytes = [0u8; 2];
        self.payload_size = 0;
        self.messages.clear();
        self.messages.push("重置索引和 buffer".into());
    }

    pub async fn listen(&mut self) {
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
                        return; // 結束循環
                    }
                };
                self.messages.push(format!("重新打開序列埠: {}", self.port_name));
                sleep(Duration::from_secs(1)); // 等待 1 秒鐘
                continue; // 重新開始循環
            }
            if read_result.is_ok() {
                self.buffer[self.idx] = buf[0];
                println!("收到 byte[{}]: {:02X}", self.idx, buf[0]);
                match self.idx {
                    0 => {
                        // 檢查 START_BYTE
                        if self.buffer[0] != START_BYTE[0] {
                            self.messages.push(
                                format!("接收到無效的 START_BYTE: {:02X?}", &self.buffer[..1])
                            );
                            self.reset().await; // 重置索引和 buffer
                            continue; // 跳過後續處理
                        }
                        self.idx += 1; // 移動到下一個字節
                        continue; // 繼續檢查下一個字節
                    }
                    _ if self.idx < 2 => {
                        self.idx += 1; // 移動到下一個字節
                        continue; // 繼續檢查下一個字節
                    }
                    _ if self.idx == 2 => {
                        // 檢查 START_BYTE
                        if
                            self.buffer[0] != START_BYTE[0] ||
                            self.buffer[1] != START_BYTE[1] ||
                            self.buffer[2] != START_BYTE[2]
                        {
                            self.messages.push(
                                format!("接收到無效的 START_BYTE: {:02X?}", &self.buffer[..3])
                            );
                            self.reset().await; // 重置索引和 buffer
                            continue; // 跳過後續處理
                        }
                        self.idx += 1; // 移動到下一個字節
                        continue; // 繼續檢查下一個字節
                    }
                    _ if self.idx == 3 => {
                        // 確認收到 CMD
                        if let Ok(c) = CMD::try_from(self.buffer[3]) {
                            cmd = c; // 確認收到 CMD
                            self.messages.push(format!("Cmd Byte: {:02X?}", cmd as u8));
                            self.idx += 1; // 移動到下一個字節
                            continue; // 繼續檢查下一個字節
                        } else {
                            self.messages.push(format!("無效的 CMD Byte: {:02X?}", self.buffer[3]));
                            self.reset().await; // 重置索引和 buffer
                            continue; // 跳過後續處理
                        }
                    }
                    _ if self.idx >= 4 && self.idx < 7 => {
                        if self.idx == 4 {
                            if let Ok(c) = Command::try_from(self.buffer[4]) {
                                command = c; // 確認收到 Command
                                self.messages.push(format!("Command Byte: {:02X?}", command as u8));
                                self.idx += 1; // 移動到下一個字節
                                continue; // 繼續檢查下一個字節
                            } else {
                                self.reset().await; // 重置索引和 buffer
                                continue; // 跳過後續處理
                            }
                        } else if self.idx == 5 {
                            self.len_bytes[0] = self.buffer[5]; // 第一個長度 byte
                            self.idx += 1; // 移動到下一個字節
                            continue; // 繼續檢查下一個字節
                        } else if self.idx == 6 {
                            self.len_bytes[1] = self.buffer[6]; // 第二個長度 byte
                            self.payload_size = u16::from_le_bytes(self.len_bytes) as usize;
                            if self.payload_size > MAX_DATA_LEN {
                                self.messages.push(
                                    format!("Payload 長度超過最大限制: {}", MAX_DATA_LEN)
                                );
                                self.reset().await; // 重置索引和 buffer
                                continue; // 跳過後續處理
                            }
                            self.messages.push(
                                format!(
                                    "Payload 長度: {} {:02X?}",
                                    self.payload_size,
                                    self.len_bytes
                                )
                            );
                            self.idx += 1; // 移動到下一個字節
                            continue; // 繼續檢查下一個字節
                        }
                    }
                    _ if self.idx >= 7 && self.idx < 7 + self.payload_size => {
                        // 接收 payload 部分
                        self.idx += 1; // 移動到下一個字節
                        continue; // 繼續檢查下一個字節
                    }
                    _ if self.idx == 7 + self.payload_size => {
                        // 確認收到第一個 CRC byte
                        self.crc_bytes[0] = self.buffer[self.idx];
                        self.idx += 1; // 移動到下一個字節
                        continue; // 繼續檢查下一個字節
                    }
                    _ if self.idx == 8 + self.payload_size => {
                        // 確認收到第二個 CRC byte
                        self.crc_bytes[1] = self.buffer[self.idx];
                        let crc = u16::from_le_bytes(self.crc_bytes);
                        self.messages.push(format!("接收到的 CRC: {:04X}", crc));
                        let calc_crc = CRC16.checksum(&self.buffer[5..7 + self.payload_size]);
                        if crc != calc_crc {
                            self.messages.push(
                                format!("CRC 校驗失敗: {:04X} != {:04X}", crc, calc_crc)
                            );
                            self.reset().await; // 重置索引和 buffer
                            continue; // 跳過後續處理
                        }
                        self.messages.push(format!("CRC 校驗成功: {:04X}", crc));
                        // 將 payload 部分轉換為 CBOR 資料
                        if command == Command::SensorHIGH || command == Command::SensorLOW {
                            // payload 不重要
                            let decoded_payload: StateMessage = serde_cbor
                                ::from_slice(&self.buffer[7..7 + self.payload_size])
                                .unwrap();
                            let elapsed = start_time.elapsed();
                            self.messages.push(
                                format!(
                                    "平均耗時: {:.2?}, 次數: {}, 接收到的資料: State: {:?}",
                                    elapsed / times,
                                    times,
                                    decoded_payload
                                )
                            );
                        } else {
                            let decoded_payload: HashMap<String, Value> = serde_cbor
                                ::from_slice(&self.buffer[7..7 + self.payload_size])
                                .unwrap();
                            let elapsed = start_time.elapsed();
                            self.messages.push(
                                format!(
                                    "平均耗時: {:.2?}, 次數: {}, 接收到的資料: Payload: {:?}",
                                    elapsed / times,
                                    times,
                                    decoded_payload
                                )
                            );
                        }
                        times += 1; // 增加次數
                        break; // 跳出循環，等待下一個資料包
                    }
                    _ => {
                        self.idx += 1; // 移動到下一個字節
                        continue; // 繼續檢查下一個字節
                    }
                }
            }
        }
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
