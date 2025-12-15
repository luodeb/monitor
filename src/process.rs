use serde::{Deserialize, Serialize};
use sysinfo::{System};
use chrono::Utc;
use indicatif::{ProgressBar, ProgressStyle};
use crate::util;

// 获取进程线程数的跨平台函数
fn get_thread_count(pid: u32) -> u32 {
    #[cfg(target_os = "linux")]
    {
        use procfs::process::Process;
        if let Ok(proc) = Process::new(pid as i32) {
            if let Ok(stat) = proc.stat() {
                return stat.num_threads as u32;
            }
        }
        1
    }
    
    #[cfg(target_os = "macos")]
    {
        use std::mem;
        use libc::{proc_pidinfo, PROC_PIDTASKALLINFO, proc_taskallinfo};
        
        unsafe {
            let mut info: proc_taskallinfo = mem::zeroed();
            let size = mem::size_of::<proc_taskallinfo>() as i32;
            let ret = proc_pidinfo(
                pid as i32,
                PROC_PIDTASKALLINFO,
                0,
                &mut info as *mut _ as *mut _,
                size,
            );
            
            if ret == size {
                return info.ptinfo.pti_threadnum as u32;
            }
        }
        1
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        1
    }
}

// 获取进程的所有线程详细信息（最多10个）
fn get_thread_details(pid: u32, process_user: &str) -> Vec<ThreadData> {
    #[cfg(target_os = "linux")]
    {
        use procfs::process::Process;
        let mut threads = Vec::new();
        
        if let Ok(proc) = Process::new(pid as i32) {
            if let Ok(tasks) = proc.tasks() {
                for task in tasks.flatten().take(10) {
                    if let Ok(stat) = task.stat() {
                        // 计算运行时间
                        let ticks_per_sec = procfs::ticks_per_second() as u64;
                        let uptime = if let Ok(boot_time) = procfs::boot_time_secs() {
                            std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs() - boot_time
                        } else {
                            0
                        };
                        
                        let start_time_secs = stat.starttime / ticks_per_sec;
                        let runtime_secs = if uptime > start_time_secs {
                            uptime - start_time_secs
                        } else {
                            0
                        };
                        
                        let hours = runtime_secs / 3600;
                        let minutes = (runtime_secs % 3600) / 60;
                        let seconds = runtime_secs % 60;
                        
                        // 读取线程状态信息获取内存数据
                        let (vsize, rss, shared) = if let Ok(status) = task.status() {
                            (
                                status.vmsize.unwrap_or(0) / 1024, // KB
                                status.vmrss.unwrap_or(0),         // KB
                                0, // 共享内存较难获取
                            )
                        } else {
                            (0, 0, 0)
                        };
                        
                        threads.push(ThreadData {
                            thread_id: stat.pid as u32,
                            user_name: process_user.to_string(),
                            priority: stat.priority as u32,
                            nice_value: stat.nice as i32,
                            virtual_memory: format_memory(vsize),
                            resident_memory: format_memory(rss),
                            shared_memory: format_memory(shared),
                            status: format!("{:?}", stat.state),
                            cpu_usage: "0.0".to_string(), // 需要采样才能计算
                            memory_usage: "0.0".to_string(),
                            runtime: format!("{}:{:02}:{:02}", hours, minutes, seconds),
                            command: stat.comm,
                        });
                    }
                }
            }
        }
        
        threads
    }
    
    #[cfg(target_os = "macos")]
    {
        // macOS 线程信息获取较复杂，这里返回空数组
        // 完整实现需要使用 task_threads 等底层API
        let _ = (pid, process_user);
        Vec::new()
    }
    
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (pid, process_user);
        Vec::new()
    }
}

// 格式化内存大小
#[cfg(target_os = "linux")]
fn format_memory(kb: u64) -> String {
    if kb >= 1024 * 1024 {
        format!("{}G", kb / (1024 * 1024))
    } else if kb >= 1024 {
        format!("{}M", kb / 1024)
    } else {
        format!("{}K", kb)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProcessData {
    #[serde(rename = "serverId")]
    pub server_id: String,
    pub pid: u32,
    pub name: String,
    #[serde(rename = "userName")]
    pub user_name: String,
    pub status: String,
    pub timestamp: u64,
    pub trend: Vec<TrendData>,
    pub threads: Vec<ThreadData>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TrendData {
    pub timestamp: u64,
    #[serde(rename = "cpuUsage")]
    pub cpu_usage: f64,
    #[serde(rename = "memoryUsage")]
    pub memory_usage: f64,
    #[serde(rename = "threadCount")]
    pub thread_count: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ThreadData {
    #[serde(rename = "threadId")]
    pub thread_id: u32,
    #[serde(rename = "userName")]
    pub user_name: String,
    pub priority: u32,
    #[serde(rename = "niceValue")]
    pub nice_value: i32,
    #[serde(rename = "virtualMemory")]
    pub virtual_memory: String,
    #[serde(rename = "residentMemory")]
    pub resident_memory: String,
    #[serde(rename = "sharedMemory")]
    pub shared_memory: String,
    pub status: String,
    #[serde(rename = "cpuUsage")]
    pub cpu_usage: String,
    #[serde(rename = "memoryUsage")]
    pub memory_usage: String,
    pub runtime: String,
    pub command: String,
}

pub fn collect_processes() -> Result<String, Box<dyn std::error::Error>> {
    // 初始化系统信息
    let mut sys = System::new_all();
    sys.refresh_all();
    
    // 生成服务器ID
    let server_id = util::generate_server_id();

    // 获取当前时间戳
    let current_timestamp = Utc::now().timestamp_millis() as u64;

    // 获取系统总内存
    let total_memory = sys.total_memory() as f64;

    // 收集所有进程信息
    let mut processes = Vec::new();
    
    // 创建进度条
    let process_count = sys.processes().len() as u64;
    let pb = ProgressBar::new(process_count);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("=>-")
    );
    
    for (pid, process) in sys.processes() {
        let process_name = process.name().to_string_lossy().to_string();
        pb.set_message(format!("处理进程: {}", process_name));
        
        // 获取用户名
        let user_name = process.user_id()
            .and_then(|uid| {
                #[cfg(unix)]
                {
                    use users::get_user_by_uid;
                    get_user_by_uid(**uid).map(|user| user.name().to_string_lossy().to_string())
                }
                #[cfg(not(unix))]
                {
                    Some(uid.to_string())
                }
            })
            .unwrap_or_else(|| "unknown".to_string());
        
        // 获取进程状态
        let status = format!("{:?}", process.status());
        
        // 计算内存使用百分比
        let memory_percentage = if total_memory > 0.0 {
            (process.memory() as f64 / total_memory) * 100.0
        } else {
            0.0
        };
        
        // 获取线程数
        let thread_count = get_thread_count(pid.as_u32());
        
        // 跳过线程数少于20的进程
        if thread_count < 20 {
            pb.inc(1);
            continue;
        }
        
        // 创建趋势数据（当前快照）
        let trend = vec![TrendData {
            timestamp: current_timestamp,
            cpu_usage: process.cpu_usage() as f64,
            memory_usage: memory_percentage,
            thread_count,
        }];
        
        // 获取线程详细信息
        let threads = get_thread_details(pid.as_u32(), &user_name);
        
        // 创建进程数据
        let process_data = ProcessData {
            server_id: server_id.clone(),
            pid: pid.as_u32(),
            name: process_name,
            user_name,
            status,
            timestamp: current_timestamp,
            trend,
            threads,
        };
        
        processes.push(process_data);
        pb.inc(1);
    }
    
    pb.finish_with_message("完成");

    // 序列化为JSON字符串（格式化输出）
    let json_string = serde_json::to_string_pretty(&processes)?;
    
    Ok(json_string)
}
