use sysinfo::System;

/// 生成唯一的服务器ID
/// 格式: hostname-machineId前8位
pub fn generate_server_id() -> String {
    // 获取主机名
    let hostname = System::host_name().unwrap_or_else(|| "unknown".to_string());

    // 获取机器唯一ID
    let machine_id = machine_uid::get().unwrap_or_else(|_| "unknown".to_string());

    // 组合生成唯一的服务器ID: hostname-machineId前8位
    format!("{}-{}", hostname, &machine_id[..8.min(machine_id.len())])
}
