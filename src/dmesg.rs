/// 采集 dmesg 数据
/// 
/// # Arguments
/// * `since_seconds` - 可选的启动后秒数，只返回此时间之后的消息
pub fn collect_dmesg(since_seconds: Option<f64>) -> Result<String, Box<dyn std::error::Error>> {
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
        
        if let Some(since) = since_seconds {
            let mut filtered_output = String::new();
            for line in stdout.lines() {
                if should_include_line(line, since) {
                    filtered_output.push_str(line);
                    filtered_output.push('\n');
                }
            }
            Ok(filtered_output)
        } else {
            Ok(stdout.to_string())
        }
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        // 非 Linux 系统返回空字符串
        Ok(String::new())
    }
}

#[cfg(target_os = "linux")]
fn should_include_line(line: &str, since_seconds: f64) -> bool {
    // dmesg 格式: [    4.396920] message...
    if !line.starts_with('[') {
        return false;
    }
    
    if let Some(end_bracket) = line.find(']') {
        let timestamp_str = &line[1..end_bracket].trim();
        if let Ok(timestamp) = timestamp_str.parse::<f64>() {
            return timestamp > since_seconds;
        }
    }
    
    false
}
