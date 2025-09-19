# language: zh-TW
# 基於 /Users/liyuefong/Desktop/JamesYeh/cbor_test/src/main.rs 和 README.md 的系統行為規格

@cbor_test @arduino_communication
Feature: CBOR Test Arduino Giga 串口通信系統
  作為一個開發者
  我想要通過串口與Arduino Giga設備通信
  以便能夠發送控制命令和接收感測器數據

  # 參考: src/main.rs 的命令行參數解析部分
  Background:
    Given 系統已安裝Rust工具鏈和相關依賴
    And Arduino Giga設備已連接並刷入相應韌體
    And 用戶有串口設備的訪問權限

  @startup @configuration
  Rule: 系統啟動和配置
    系統需要正確處理命令行參數和初始化配置

    Scenario: 使用默認配置啟動應用程序
      When 用戶執行命令 "cargo run -- /dev/tty.usbmodem1234561"
      Then 系統應該使用串口 "/dev/tty.usbmodem1234561"
      And 波特率應該設置為默認值
      And 調試模式應該為關閉狀態
      And 超時時間應該設置為 0.0005 秒
      And 系統應該創建日誌目錄 "logs/"

    Scenario: 使用自定義配置啟動應用程序
      When 用戶執行命令 "cargo run -- COM5 debug=true show_byte=true timeout=0.001"
      Then 系統應該使用串口 "COM5"
      And 調試模式應該為開啟狀態
      And 字節顯示模式應該為開啟狀態
      And 超時時間應該設置為 0.001 秒
      And 日誌級別應該設置為 "debug"

    Scenario: 配置Giga消息顯示參數
      When 用戶執行命令 "cargo run -- /dev/tty.usb show_giga=true show_giga_interval=0.5"
      Then Giga消息顯示應該為開啟狀態
      And Giga消息顯示間隔應該設置為 0.5 秒
      And 系統應該按照指定間隔顯示來自設備的實時數據

  @logging @monitoring
  Rule: 日誌記錄和監控系統
    系統需要提供詳細的日誌記錄和實時監控功能

    Scenario: 日誌系統初始化
      Given 系統啟動時
      When 日誌系統被初始化
      Then 應該創建控制台輸出層和文件輸出層
      And 文件日誌應該保存在 "logs/cbor_test.log.YYYY-MM-DD" 格式
      And 控制台日誌應該包含顏色編碼和線程ID
      And 文件日誌應該包含詳細的調試信息和時間戳

    Scenario: 調試模式下的詳細日誌
      Given 系統以 "debug=true" 參數啟動
      When 系統執行任何操作
      Then 日誌級別應該設置為 "debug"
      And 應該記錄詳細的CBOR和COBS協議信息
      And 應該顯示原始字節數據 (如果 show_byte=true)

  @protocol @cbor_cobs
  Rule: CBOR/COBS 協議處理
    系統需要正確實現CBOR序列化和COBS幀封裝

    # 參考: src/main.rs 第140-220行的payload測試部分
    Scenario: CBOR序列化測試
      Given 有一個 StateMessage payload 包含 "status: 0"
      When 系統將payload序列化為CBOR格式
      Then CBOR數據應該是有效的二進制格式
      And 應該記錄CBOR數據的十六進制表示和大小
      And 序列化過程應該沒有錯誤

    Scenario: COBS幀封裝測試
      Given 有一個已序列化的CBOR payload
      When 系統使用Action::SEND和Command::Motor創建COBS幀
      Then 應該生成包含CRC校驗的COBS幀
      And 應該記錄幀大小和CRC值
      And 完整發送幀應該以0x00字節開始

    Scenario: COBS解碼和消息解析測試
      Given 有一個COBS編碼的幀
      When 系統解碼COBS幀
      Then 應該成功恢復原始數據
      And 解碼的消息應該包含正確的Action和Command
      And 應該正確解析payload字節和CRC校驗

  @serial_communication @connection_management
  Rule: 串口通信和連接管理
    系統需要穩定地管理與Arduino設備的串口連接

    Scenario: 串口設備掃描和連接
      Given 系統啟動時
      When 系統掃描可用的串口設備
      Then 應該列出所有可用的串口名稱
      And 應該嘗試連接到指定的串口
      And 連接成功時應該設置連接狀態為true

    Scenario: 串口連接失敗處理
      Given 指定的串口設備不存在或無法訪問
      When 系統嘗試連接
      Then 應該記錄連接錯誤信息
      And 連接狀態應該保持為false
      And 系統應該提示用戶檢查設備連接

    Scenario: 自動重連機制
      Given Arduino設備連接正常
      When 連接意外中斷
      Then 系統應該檢測到連接丟失
      And 應該記錄 "Giga connection lost" 警告
      And 應該提示用戶使用 "/r" 或 "r" 命令重新連接
      And 用戶可以通過重連命令恢復連接

  @command_processing @json_interface
  Rule: 命令處理和JSON接口
    系統需要正確解析和處理用戶的JSON命令

    # 參考: src/main.rs 的MotorCommandParams結構和命令處理循環
    Scenario: 有效的電機控制命令處理
      Given 系統正在等待用戶輸入
      When 用戶輸入JSON命令:
        """
        {"action": "SEND", "cmd": "Motor", "payload": {"PMt": {"id": 1, "motion": 1, "rpm": 500, "acc": 0, "volt": 0, "temp": 0, "amp": 0}}}
        """
      Then 系統應該成功解析JSON為MotorCommandParams結構
      And 命令應該被加入發送隊列
      And 應該記錄 "Command queued" 信息
      And 如果設備已連接，命令應該通過COBS協議發送

    Scenario: 感測器讀取命令處理
      Given 系統正在等待用戶輸入
      When 用戶輸入感測器讀取命令:
        """
        {"action": "READ", "cmd": "Sensor", "payload": {"name": "SRc"}}
        """
      Then 系統應該解析為READ動作
      And 命令類型應該為Sensor
      And 應該向設備發送讀取請求

    Scenario: 無效JSON命令處理
      Given 系統正在等待用戶輸入
      When 用戶輸入格式錯誤的JSON: "{"action": "INVALID""
      Then 系統應該記錄 "Invalid JSON" 錯誤
      And 應該顯示錯誤詳情
      And 系統應該繼續等待下一個命令

  @interactive_commands @control_interface
  Rule: 互動式命令和控制接口
    系統需要提供用戶友好的控制命令接口

    Scenario: 退出程序命令
      Given 系統正在運行
      When 用戶輸入 "q" 或 "/q"
      Then 系統應該設置退出標誌
      And 應該優雅地關閉Giga連接
      And 應該記錄 "CBOR Test Complete" 信息
      And 程序應該正常退出

    Scenario: 手動重連命令
      Given Arduino設備連接已中斷
      When 用戶輸入 "r" 或 "/r"
      Then 系統應該發送重連信號
      And 應該嘗試重新建立與設備的連接
      And 成功重連後應該恢復正常通信

    # 參考: src/main.rs 第440-470行的測試命令生成邏輯
    Scenario: 自動測試命令生成
      Given 系統正在等待用戶輸入
      When 用戶輸入 "/t=3"
      Then 系統應該生成包含3個電機payload的測試JSON
      And 每個payload應該包含完整的標籤列表: ["id", "motion", "rpm", "tol", "dist", "angle", "time", "acc", "newid", "volt", "amp", "temp", "mode", "status"]
      And 生成的JSON應該使用READ動作和Motor命令
      And 應該記錄生成的測試JSON內容
      And JSON應該被自動處理為正常命令

    Scenario: 測試命令參數驗證
      Given 系統正在等待用戶輸入
      When 用戶輸入無效的測試命令 "/t=abc"
      Then 系統應該記錄 "Invalid test count value" 錯誤
      And 應該將輸入作為普通字符串處理
      And 不應該生成測試JSON

  @async_processing @background_tasks
  Rule: 異步處理和後台任務
    系統需要使用多個異步任務來處理並發操作

    Scenario: 後台設備監聽任務
      Given 系統啟動並連接到Arduino設備
      When 後台任務開始運行
      Then 應該持續監聽來自設備的消息
      And 應該處理重連請求
      And 應該發送排隊的命令到設備
      And 當設備觸發時應該記錄狀態變化

    Scenario: 主任務用戶輸入處理
      Given 系統正在運行
      When 主任務處理用戶輸入
      Then 應該非阻塞地讀取stdin
      And 應該解析JSON命令
      And 應該通過通道發送命令到後台任務
      And 應該繼續等待下一個輸入

    Scenario: 任務間通信機制
      Given 主任務和後台任務都在運行
      When 需要在任務間傳遞數據
      Then 應該使用tokio::sync::mpsc通道
      And giga_send_tx通道應該傳遞MotorCommandParams
      And giga_reconnect_tx通道應該傳遞重連請求
      And 通道容量應該設置為128條消息

  @real_time_monitoring @giga_messages
  Rule: 實時監控和Giga消息處理
    系統需要提供可配置的實時數據監控功能

    Scenario: Giga消息過濾和顯示
      Given 系統啟動時設置 "show_giga=true"
      And 設置 "show_giga_interval=0.1"
      When 從設備接收到Action::GIGA類型的消息
      Then 應該按照0.1秒間隔限制顯示頻率
      And 應該使用LAST_GIGA_LOG來控制顯示時間
      And 應該記錄消息的payload內容

    Scenario: 非Giga消息處理
      Given 系統正在監聽設備消息
      When 接收到Action::SEND或Action::READ類型的消息
      Then 應該立即記錄完整的消息響應
      And 不應該受到show_giga_interval的限制
      And 應該顯示消息的action和完整內容

    Scenario: 發送消息日誌記錄
      Given 系統配置為顯示字節數據
      When 向設備發送CBOR或COBS數據
      Then 應該記錄 "Send CBOR" 和數據長度
      And 應該記錄 "Send COBS" 和數據長度
      And 應該以十六進制格式顯示原始字節數據

  @error_handling @robustness
  Rule: 錯誤處理和系統健壯性
    系統需要優雅地處理各種錯誤情況

    Scenario: 設備通信錯誤處理
      Given Arduino設備已連接
      When 設備通信過程中發生錯誤
      Then 系統應該捕獲錯誤並記錄詳細信息
      And 應該設置設備退出標誌
      And 應該更新連接狀態為false
      And 應該提示用戶重新連接

    Scenario: 命令發送失敗處理
      Given 系統嘗試向設備發送命令
      When 發送過程中發生錯誤
      Then 應該記錄 "Failed to send Giga Data" 錯誤
      And 應該包含具體的錯誤信息
      And 系統應該繼續處理其他命令

    Scenario: 通道通信錯誤處理
      Given 任務間通過通道通信
      When 通道發送失敗
      Then 應該記錄具體的通道錯誤信息
      And 系統應該嘗試恢復或提示用戶

  @supported_actions_commands
  Rule: 支持的動作和命令類型
    系統需要正確處理所有定義的動作和命令類型

    # 參考: src/main.rs 第290-300行的支持類型列表
    Scenario: 支持的Action類型處理
      Given 系統接收到JSON命令
      When Action字段為以下任一值: "SEND", "READ", "GIGA"
      Then 系統應該正確解析並處理對應的動作
      And 每種動作應該有相應的處理邏輯

    Scenario: 支持的Command類型處理
      Given 系統接收到JSON命令
      When Command字段為以下任一值: "Ack", "NAck", "Motor", "Sensor", "File"
      Then 系統應該正確解析並處理對應的命令類型
      And 每種命令應該有相應的payload格式要求

  @payload_structure @data_validation
  Rule: Payload結構和數據驗證
    系統需要正確處理不同類型的payload數據

    # 參考: src/main.rs 第31-50行的payload結構定義
    Scenario: SetMotorPayload結構驗證
      Given 用戶發送包含電機控制數據的命令
      When payload包含SetMotorPayload結構
      Then 應該包含以下字段: id (u8), motion (u8), rpm (i64), acc (u64), volt (f32), temp (f32), amp (f32)
      And 所有字段應該能夠正確序列化為CBOR格式
      And 數據類型應該與定義匹配

    Scenario: 讀取命令payload處理
      Given 用戶發送讀取類型的命令
      When payload為Read類型 (HashMap<String, Value>)
      Then 系統應該正確解析鍵值對結構
      And 應該支持動態的標籤和值組合
      And 應該能夠處理測試命令生成的多個payload項

  @system_lifecycle @graceful_shutdown
  Rule: 系統生命週期和優雅關閉
    系統需要正確管理啟動、運行和關閉流程

    Scenario: 系統完整生命週期
      Given 用戶啟動cbor_test應用程序
      When 系統執行完整的運行週期
      Then 應該按順序執行: 參數解析 → 日誌初始化 → 協議測試 → 設備連接 → 用戶交互循環
      And 每個階段都應該有相應的日誌記錄
      And 最終應該記錄 "🎊 CBOR Test Complete" 完成信息

    Scenario: 優雅關閉流程
      Given 系統正在運行並連接到設備
      When 用戶發送退出命令
      Then 應該設置exit_flag為true
      And 應該通知後台任務停止
      And 應該關閉與Arduino設備的連接
      And 應該等待所有異步任務完成
      And 應該確保日誌緩衝區被完全刷新