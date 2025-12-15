use serde::{Deserialize, Serialize};
use chrono::Utc;
use crate::util;

#[derive(Debug, Serialize, Deserialize)]
pub struct DmesgResponse {
    #[serde(rename = "serverId")]
    pub server_id: String,
    pub timestamp: u64,
    pub entries: Vec<DmesgEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DmesgEntry {
    pub timestamp: u64,
    pub level: String,
    pub message: String,
}

/// 采集 dmesg 数据
/// 
/// # Arguments
/// * `since_seconds` - 可选的启动后秒数，只返回此时间之后的消息
pub fn collect_dmesg(since_seconds: Option<f64>) -> Result<String, Box<dyn std::error::Error>> {
    // 生成服务器ID
    let server_id = util::generate_server_id();
    
    // 获取当前时间戳（毫秒）
    let current_timestamp = Utc::now().timestamp_millis() as u64;
    
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        
        // 在 Linux 上执行 dmesg 命令（不使用 -T 和 -x，直接获取原始格式）
        let output = Command::new("dmesg")
            .output()?;
        
        if !output.status.success() {
            return Err(format!("dmesg command failed: {}", 
                String::from_utf8_lossy(&output.stderr)).into());
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let entries = parse_dmesg_output(&stdout, since_seconds)?;
        
        let response = DmesgResponse {
            server_id,
            timestamp: current_timestamp,
            entries,
        };
        
        // 序列化为JSON字符串（格式化输出）
        let json_string = serde_json::to_string_pretty(&response)?;
        Ok(json_string)
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        // 非 Linux 系统返回空数据
        let response = DmesgResponse {
            server_id,
            timestamp: current_timestamp,
            entries: Vec::new(),
        };
        
        let json_string = serde_json::to_string_pretty(&response)?;
        Ok(json_string)
    }
}

#[cfg(target_os = "linux")]
fn parse_dmesg_output(output: &str, since_seconds: Option<f64>) -> Result<Vec<DmesgEntry>, Box<dyn std::error::Error>> {
    let mut entries = Vec::new();
    
    // 获取系统启动时间，用于计算每个日志条目的真实时间戳
    let boot_time = get_boot_time()?;
    
    for line in output.lines() {
        if line.trim().is_empty() {
            continue;
        }
        
        // dmesg 默认输出格式示例：
        // [    4.396920] acpi power operation succeed
        // [   14.353120] snd_hda_codec_conexant hdaudioC0D0: cx_auto_init.
        
        if let Some(entry) = parse_dmesg_line(line, boot_time) {
            // 如果提供了启动后秒数过滤，只保留更新的消息
            if let Some(since) = since_seconds {
                // 从时间戳反推启动后秒数进行比较
                let entry_boot_seconds = (entry.timestamp - boot_time) as f64 / 1000.0;
                if entry_boot_seconds <= since {
                    continue;
                }
            }
            entries.push(entry);
        }
    }
    
    Ok(entries)
}

#[cfg(target_os = "linux")]
fn parse_dmesg_line(line: &str, boot_time: u64) -> Option<DmesgEntry> {
    // 查找时间戳部分 [    4.396920]
    if !line.starts_with('[') {
        return None;
    }
    
    let end_bracket = line.find(']')?;
    let timestamp_str = &line[1..end_bracket].trim();
    
    // 解析启动后的秒数
    let boot_seconds: f64 = timestamp_str.parse().ok()?;
    
    // 计算实际时间戳（毫秒）
    let timestamp = boot_time + (boot_seconds * 1000.0) as u64;
    
    // 提取消息内容
    let message = line[end_bracket + 1..].trim().to_string();
    
    // 根据消息内容推断日志级别
    let level = if message.contains("error") || message.contains("Error") || message.contains("ERROR") {
        "error"
    } else if message.contains("warn") || message.contains("Warn") || message.contains("WARN") {
        "warning"
    } else if message.contains("critical") || message.contains("CRITICAL") || message.contains("crit") {
        "critical"
    } else {
        "info"
    };
    
    Some(DmesgEntry {
        timestamp,
        level: level.to_string(),
        message,
    })
}

#[cfg(target_os = "linux")]
fn get_boot_time() -> Result<u64, Box<dyn std::error::Error>> {
    use std::fs;
    
    // 从 /proc/uptime 读取系统运行时间
    let uptime_str = fs::read_to_string("/proc/uptime")?;
    let uptime_parts: Vec<&str> = uptime_str.split_whitespace().collect();
    
    if let Some(uptime_str) = uptime_parts.first() {
        let uptime_seconds: f64 = uptime_str.parse()?;
        let current_time = Utc::now().timestamp_millis() as u64;
        let boot_time = current_time - (uptime_seconds * 1000.0) as u64;
        return Ok(boot_time);
    }
    
    Err("Failed to parse /proc/uptime".into())
}

#[cfg(target_os = "linux")]
fn _parse_timestamp(timestamp_str: &str) -> Result<u64, Box<dyn std::error::Error>> {
    use chrono::{DateTime, NaiveDateTime, TimeZone};
    
    // dmesg -T 格式: "Mon Dec 16 10:30:45 2024"
    // 我们需要解析这种格式
    
    // 尝试使用 chrono 解析
    if let Ok(dt) = DateTime::parse_from_str(timestamp_str, "%a %b %e %H:%M:%S %Y") {
        return Ok(dt.timestamp_millis() as u64);
    }
    
    // 尝试另一种格式
    if let Ok(naive_dt) = NaiveDateTime::parse_from_str(timestamp_str, "%a %b %e %H:%M:%S %Y") {
        let dt = Utc.from_utc_datetime(&naive_dt);
        return Ok(dt.timestamp_millis() as u64);
    }
    
    // 如果解析失败，返回当前时间
    Ok(Utc::now().timestamp_millis() as u64)
}
