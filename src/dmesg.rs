/// 采集 dmesg 数据
/// 
/// # Arguments
/// * `since_seconds` - 可选的启动后秒数，只返回此时间之后的消息
/// 
/// # Returns
/// * `(String, Option<f64>)` - (日志内容, 最后一条日志的时间戳)
pub fn collect_dmesg(since_seconds: Option<f64>) -> Result<(String, Option<f64>), Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        
        // 在 Linux 上执行 dmesg 命令
        let output = Command::new("dmesg")
            .output()?;
        
        if !output.status.success() {
            return Err(format!("dmesg command failed: {}", 
                String::from_utf8_lossy(&output.stderr)).into());
        }
        
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut last_timestamp = since_seconds;
        
        if let Some(since) = since_seconds {
            let mut filtered_output = String::new();
            for line in stdout.lines() {
                if let Some(ts) = parse_timestamp(line) {
                    if ts > since {
                        filtered_output.push_str(line);
                        filtered_output.push('\n');
                        last_timestamp = Some(ts);
                    }
                }
            }
            Ok((filtered_output, last_timestamp))
        } else {
            // 如果没有指定起始时间，返回所有日志，并找到最后的时间戳
            for line in stdout.lines() {
                if let Some(ts) = parse_timestamp(line) {
                    last_timestamp = Some(ts);
                }
            }
            Ok((stdout.to_string(), last_timestamp))
        }
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        let _ = since_seconds;
        // 非 Linux 系统返回空字符串
        Ok((String::new(), None))
    }
}

#[cfg(target_os = "linux")]
fn parse_timestamp(line: &str) -> Option<f64> {
    // dmesg 格式: [    4.396920] message...
    if !line.starts_with('[') {
        return None;
    }
    
    if let Some(end_bracket) = line.find(']') {
        let timestamp_str = &line[1..end_bracket].trim();
        return timestamp_str.parse::<f64>().ok();
    }
    
    None
}
