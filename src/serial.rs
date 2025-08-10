use std::time::Duration;
use anyhow::Error;
#[allow(unused_imports)]
use tracing::{ info, error, debug, warn };

pub async fn open_serial_port(
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
                // sleep(Duration::from_secs(1)).await;
                continue; // 重新嘗試打開序列埠
            }
        }
    }
}
