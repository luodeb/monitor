use axum::{
    Json, Router,
    routing::post,
};
use tower_http::cors::CorsLayer;
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use tokio::net::TcpListener;

mod dmesg;
mod metrics;
mod process;
mod util;

#[derive(Parser)]
#[command(name = "monitor")]
#[command(about = "系统监控工具", long_about = None)]
struct Cli {
    /// 启动后端服务器的端口
    #[arg(long, global = true)]
    server: Option<u16>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 收集并输出进程信息
    Process {
        /// 检查线程数最多的进程
        #[arg(long)]
        check: bool,
    },
    /// 收集并输出系统指标信息
    Metrics,
    /// 收集并输出 dmesg 日志
    Dmesg {
        /// 只获取此时间之后的日志（启动后秒数，例如：4.5）
        #[arg(long)]
        since: Option<f64>,
    },
    /// 持续监控并输出信息
    Monitor {
        /// 间隔分钟数
        #[arg(long)]
        min: Option<u64>,
        /// 间隔秒数
        #[arg(long)]
        sec: Option<u64>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    if let Some(port) = cli.server {
        tokio::spawn(async move {
            if let Err(e) = start_server(port).await {
                eprintln!("Server error: {}", e);
            }
        });
    }

    if let Some(command) = cli.command {
        match command {
            Commands::Process { check } => {
                if check {
                    let json = process::check_max_threads_process()?;
                    println!("{}", json);
                } else {
                    let json = process::collect_processes()?;
                    println!("{}", json);
                }
            }
            Commands::Metrics => {
                let json = metrics::collect_metrics()?;
                println!("{}", json);
            }
            Commands::Dmesg { since } => {
                let (json, _) = dmesg::collect_dmesg(since)?;
                println!("{}", json);
            }
            Commands::Monitor { min, sec } => {
                let interval_secs = min.unwrap_or(0) * 60 + sec.unwrap_or(0);
                if interval_secs == 0 {
                    return Err("Please specify an interval using --min or --sec".into());
                }

                let mut last_dmesg_time: Option<f64> = None;

                loop {
                    println!("--- Monitor Loop Start ---");
                    match metrics::collect_metrics() {
                        Ok(json) => println!("Metrics: {}", json),
                        Err(e) => eprintln!("Error collecting metrics: {}", e),
                    }

                    match process::collect_processes() {
                        Ok(json) => println!("Process: {}", json),
                        Err(e) => eprintln!("Error collecting processes: {}", e),
                    }

                    match dmesg::collect_dmesg(last_dmesg_time) {
                        Ok((json, new_last_time)) => {
                            if !json.is_empty() {
                                println!("Dmesg: {}", json);
                            }
                            if let Some(t) = new_last_time {
                                last_dmesg_time = Some(t);
                            }
                        }
                        Err(e) => eprintln!("Error collecting dmesg: {}", e),
                    }
                    println!("--- Monitor Loop End ---");

                    tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
                }
            }
        }
    } else if cli.server.is_none(){
        use clap::CommandFactory;
        Cli::command().print_help()?;
    }

    if cli.server.is_some() {
        // Wait forever if only server is running
        std::future::pending::<()>().await;
    }

    Ok(())
}

async fn start_server(port: u16) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let app = Router::new()
        .route("/api/getAllData", post(get_all_data).get(get_all_data))
        .layer(CorsLayer::permissive());
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    println!("Server listening on {}", addr);
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn get_all_data() -> Json<serde_json::Value> {
    match std::fs::read_to_string("data.json") {
        Ok(content) => {
            let v: serde_json::Value =
                serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
            Json(v)
        }
        Err(_) => Json(serde_json::json!({})),
    }
}
