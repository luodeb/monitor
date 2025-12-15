use serde::{Deserialize, Serialize};
use chrono::Utc;
use sysinfo::{System, Disks, Networks};
use crate::util;

#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsData {
    #[serde(rename = "serverId")]
    pub server_id: String,
    pub timestamp: u64,
    #[serde(rename = "cpuUsage")]
    pub cpu_usage: f64,
    #[serde(rename = "memoryUsage")]
    pub memory_usage: f64,
    #[serde(rename = "diskUsage")]
    pub disk_usage: f64,
    #[serde(rename = "ioRead")]
    pub io_read: f64,
    #[serde(rename = "ioWrite")]
    pub io_write: f64,
    #[serde(rename = "networkIn")]
    pub network_in: f64,
    #[serde(rename = "networkOut")]
    pub network_out: f64,
}

pub fn collect_metrics() -> Result<String, Box<dyn std::error::Error>> {
    // 生成服务器ID
    let server_id = util::generate_server_id();
    
    // 获取当前时间戳（毫秒）
    let timestamp = Utc::now().timestamp_millis() as u64;
    
    // 初始化系统信息
    let mut sys = System::new_all();
    sys.refresh_all();
    
    // 等待一小段时间后再次刷新，以获取准确的CPU使用率
    std::thread::sleep(std::time::Duration::from_millis(200));
    sys.refresh_cpu_all();
    
    // 计算CPU使用率（所有核心的平均值）
    let cpu_usage = sys.cpus().iter()
        .map(|cpu| cpu.cpu_usage() as f64)
        .sum::<f64>() / sys.cpus().len() as f64;
    
    // 计算内存使用率
    let total_memory = sys.total_memory() as f64;
    let used_memory = sys.used_memory() as f64;
    let memory_usage = if total_memory > 0.0 {
        (used_memory / total_memory) * 100.0
    } else {
        0.0
    };
    
    // 计算磁盘使用率
    let disks = Disks::new_with_refreshed_list();
    let (total_disk, used_disk) = disks.iter().fold((0u64, 0u64), |(total, used), disk| {
        let disk_total = disk.total_space();
        let disk_available = disk.available_space();
        let disk_used = disk_total.saturating_sub(disk_available);
        (total + disk_total, used + disk_used)
    });
    
    let disk_usage = if total_disk > 0 {
        (used_disk as f64 / total_disk as f64) * 100.0
    } else {
        0.0
    };
    
    // 获取网络统计信息
    let networks = Networks::new_with_refreshed_list();
    std::thread::sleep(std::time::Duration::from_millis(100));
    let mut networks = networks;
    networks.refresh();
    
    let (network_in, network_out) = networks.iter().fold((0u64, 0u64), |(rx, tx), (_, data)| {
        (rx + data.received(), tx + data.transmitted())
    });
    
    // 转换为 KB/s (除以1024)
    let network_in_kb = network_in as f64 / 1024.0;
    let network_out_kb = network_out as f64 / 1024.0;
    
    // IO读写数据（Linux特定）
    let (io_read, io_write) = get_io_stats();
    
    let metrics = MetricsData {
        server_id,
        timestamp,
        cpu_usage: (cpu_usage * 10.0).round() / 10.0, // 保留一位小数
        memory_usage: (memory_usage * 10.0).round() / 10.0,
        disk_usage: (disk_usage * 10.0).round() / 10.0,
        io_read,
        io_write,
        network_in: (network_in_kb * 10.0).round() / 10.0,
        network_out: (network_out_kb * 10.0).round() / 10.0,
    };
    
    // 将单个指标数据包装在数组中
    let metrics_array = vec![metrics];
    
    // 序列化为JSON字符串（格式化输出）
    let json_string = serde_json::to_string_pretty(&metrics_array)?;
    
    Ok(json_string)
}

// 获取IO统计信息
fn get_io_stats() -> (f64, f64) {
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        
        if let Ok(content) = fs::read_to_string("/proc/diskstats") {
            let (total_read, total_write) = content.lines()
                .filter_map(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 14 {
                        // 只统计主要设备（不包括分区）
                        let device_name = parts[2];
                        if !device_name.chars().last().unwrap_or('0').is_ascii_digit() {
                            // sectors read (字段6) 和 sectors written (字段10)
                            // 每个扇区通常是512字节
                            let read_sectors = parts[5].parse::<u64>().ok()?;
                            let write_sectors = parts[9].parse::<u64>().ok()?;
                            return Some((read_sectors, write_sectors));
                        }
                    }
                    None
                })
                .fold((0u64, 0u64), |(r, w), (read, write)| (r + read, w + write));
            
            // 转换为MB (扇区 * 512 / 1024 / 1024)
            let read_mb = (total_read as f64 * 512.0) / (1024.0 * 1024.0);
            let write_mb = (total_write as f64 * 512.0) / (1024.0 * 1024.0);
            
            return ((read_mb * 10.0).round() / 10.0, (write_mb * 10.0).round() / 10.0);
        }
        (0.0, 0.0)
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        // macOS 和其他系统暂时返回0
        (0.0, 0.0)
    }
}
