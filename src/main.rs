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
            let json = dmesg::collect_dmesg(since)?;
            println!("{}", json);
        }
    }

    Ok(())
}
