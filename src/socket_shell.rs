use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use axum::{
    extract::{State, ws::{WebSocketUpgrade, WebSocket, Message}},
    response::Response,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use serde_json;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct TerminalMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub data: String,
    pub timestamp: i64,
    pub system_info: Option<SystemInfo>,
}

#[derive(Serialize, Deserialize)]
pub struct SystemInfo {
    pub pwd: String,
    pub user: String,
    pub hostname: String,
    pub shell: String,
}

pub type Sessions = Arc<Mutex<HashMap<String, mpsc::UnboundedSender<String>>>>;

pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(sessions): State<Sessions>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, sessions))
}

pub async fn handle_socket(socket: WebSocket, sessions: Sessions) {
    let session_id = Uuid::new_v4().to_string();
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let (input_tx, input_rx) = mpsc::unbounded_channel::<String>();
    
    // 存储会话
    {
        let mut sessions_lock = sessions.lock().await;
        sessions_lock.insert(session_id.clone(), tx.clone());
    }
    
    // 启动本地Shell连接
    let shell_tx = tx.clone();
    let shell_handle = tokio::spawn(async move {
        if let Err(e) = start_local_shell(shell_tx, input_rx).await {
            eprintln!("Shell connection error: {}", e);
        }
    });
    
    // 处理从Shell接收的数据并发送到WebSocket
    let sessions_clone = sessions.clone();
    let session_id_clone = session_id.clone();
    let output_handle = tokio::spawn(async move {
        while let Some(data) = rx.recv().await {
            // 检查是否是JSON格式的消息（来自其他处理器）
            if data.starts_with("{") && data.ends_with("}") {
                // 直接发送JSON消息
                if sender.send(Message::Text(data)).await.is_err() {
                    break;
                }
            } else {
                // 包装为标准输出消息
                let msg = TerminalMessage {
                    msg_type: "output".to_string(),
                    data,
                    timestamp: chrono::Utc::now().timestamp(),
                    system_info: None,
                };
                if let Ok(json) = serde_json::to_string(&msg) {
                    if sender.send(Message::Text(json)).await.is_err() {
                        break;
                    }
                }
            }
        }
        
        // 清理会话
        let mut sessions_lock = sessions_clone.lock().await;
        sessions_lock.remove(&session_id_clone);
    });
    
    // 处理从WebSocket接收的输入
    let input_handle = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            if let Ok(msg) = msg {
                if let Message::Text(text) = msg {
                    if let Ok(terminal_msg) = serde_json::from_str::<TerminalMessage>(&text) {
                        match terminal_msg.msg_type.as_str() {
                            "input" => {
                                let _ = input_tx.send(terminal_msg.data);
                            }
                            "request_system_info" => {
                                // 请求系统信息，发送pwd命令
                                let _ = input_tx.send("echo \"__SYSTEM_INFO__:$(pwd):$(whoami):$(hostname):$SHELL\"\n".to_string());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    });
    
    // 等待任一任务完成
    tokio::select! {
        _ = shell_handle => {},
        _ = output_handle => {},
        _ = input_handle => {},
    }
}

async fn start_local_shell(
    output_tx: mpsc::UnboundedSender<String>,
    mut input_rx: mpsc::UnboundedReceiver<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use tokio::process::Command;
    use tokio::io::{AsyncWriteExt, AsyncReadExt};
    
    // 获取用户主目录
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
    
    // 启动本地shell进程 - 使用非交互式模式避免ANSI转义序列
    let mut child = Command::new("/bin/bash")
        .current_dir(&home_dir) // 设置工作目录为用户主目录
        .env("TERM", "dumb") // 设置为dumb终端，避免颜色和控制字符
        .env("PS1", "$ ") // 简单的提示符
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    
    // 处理stdout
    let output_tx_clone = output_tx.clone();
    let stdout_handle = tokio::spawn(async move {
        let mut stdout = stdout;
        let mut buffer = [0; 1024];
        loop {
            match stdout.read(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let output = String::from_utf8_lossy(&buffer[..n]);
                    println!("stdout: {}", output);
                    
                    // 检查是否包含系统信息
                    if output.contains("__SYSTEM_INFO__:") {
                        // 提取系统信息并发送特殊消息
                        if let Some(info_line) = output.lines().find(|line| line.contains("__SYSTEM_INFO__:")) {
                            let parts: Vec<&str> = info_line.split(':').collect();
                            if parts.len() >= 5 {
                                let system_info = SystemInfo {
                                    pwd: parts[1].trim().to_string(),
                                    user: parts[2].trim().to_string(),
                                    hostname: parts[3].trim().to_string(),
                                    shell: parts[4].trim().to_string(),
                                };
                                
                                let system_msg = TerminalMessage {
                                    msg_type: "system".to_string(),
                                    data: "System info updated".to_string(),
                                    timestamp: chrono::Utc::now().timestamp(),
                                    system_info: Some(system_info),
                                };
                                
                                if let Ok(json) = serde_json::to_string(&system_msg) {
                                    let _ = output_tx_clone.send(json);
                                }
                                continue; // 不发送原始输出
                            }
                        }
                    }
                    
                    // 发送普通输出消息
                    let output_msg = TerminalMessage {
                        msg_type: "output".to_string(),
                        data: output.to_string(),
                        timestamp: chrono::Utc::now().timestamp(),
                        system_info: None,
                    };
                    
                    if let Ok(json) = serde_json::to_string(&output_msg) {
                        if output_tx_clone.send(json).is_err() {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });
    
    // 处理stderr
    let output_tx_clone2 = output_tx.clone();
    let stderr_handle = tokio::spawn(async move {
        let mut stderr = stderr;
        let mut buffer = [0; 1024];
        loop {
            match stderr.read(&mut buffer).await {
                Ok(0) => break, // EOF
                Ok(n) => {
                    let output = String::from_utf8_lossy(&buffer[..n]);
                    println!("stderr: {}", output);
                    
                    // 发送错误输出消息
                    let output_msg = TerminalMessage {
                        msg_type: "output".to_string(),
                        data: output.to_string(),
                        timestamp: chrono::Utc::now().timestamp(),
                        system_info: None,
                    };
                    
                    if let Ok(json) = serde_json::to_string(&output_msg) {
                        if output_tx_clone2.send(json).is_err() {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
    });
    
    // 处理输入
    let home_dir_clone = home_dir.clone();
    let input_handle = tokio::spawn(async move {
        // 发送初始化命令
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        if stdin.write_all(format!("cd {}\n", home_dir_clone).as_bytes()).await.is_ok() {
            let _ = stdin.flush().await;
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            if stdin.write_all(b"echo \"__SYSTEM_INFO__:$(pwd):$(whoami):$(hostname):$SHELL\"\n").await.is_ok() {
                let _ = stdin.flush().await;
            }
        }
        
        while let Some(input) = input_rx.recv().await {
            println!("User input: {input}");
            if stdin.write_all(input.as_bytes()).await.is_err() {
                break;
            }
            if stdin.flush().await.is_err() {
                break;
            }
            
            // 如果是回车，自动获取系统信息
            if input.contains('\n') {
                // 等待一小段时间让命令执行完
                tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                if stdin.write_all(b"echo \"__SYSTEM_INFO__:$(pwd):$(whoami):$(hostname):$SHELL\"\n").await.is_err() {
                    break;
                }
                if stdin.flush().await.is_err() {
                    break;
                }
            }
        }
    });
    
    // 等待任一任务完成
    tokio::select! {
        _ = stdout_handle => {},
        _ = stderr_handle => {},
        _ = input_handle => {},
        _ = child.wait() => {},
    }
    
    Ok(())
}