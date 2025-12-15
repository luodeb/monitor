use clap::{Parser, Subcommand};

mod process;
mod metrics;
mod dmesg;
mod util;

#[derive(Parser)]
#[command(name = "monitor")]
#[command(about = "系统监控工具", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 收集并输出进程信息
    Process,
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Process => {
            let json = process::collect_processes()?;
            println!("{}", json);
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

                std::thread::sleep(std::time::Duration::from_secs(interval_secs));
            }
        }
    }

    Ok(())
}
