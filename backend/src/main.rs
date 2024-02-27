use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use log::error;
use rand::Rng;
use regex::Regex;
use serde::Deserialize;
use std::{net::SocketAddr, path::PathBuf};
use warp::Filter;

mod config;
mod connection;
mod json_rpc;
mod parser;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate random log file of specified length
    Babble(BabbleArgs),
    /// Run the log server
    Server(ServerArgs),
    /// Test tailing a log file
    Tail(TailArgs),
}

#[derive(Args)]
struct BabbleArgs {
    #[arg(long, short_alias = 'n', default_value = "100")]
    lines: usize,
}

#[derive(Args)]
struct ServerArgs {
    #[arg(long, default_value = "127.0.0.1:9000")]
    bind: SocketAddr,
}

#[derive(Args, Debug, Clone, Deserialize)]
struct TailArgs {
    #[arg(long)]
    cols: usize,
    #[arg(long)]
    filter: Option<String>,
    log_file: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Babble(args) => babble(args)?,
        Command::Server(args) => server(args).await?,
        Command::Tail(args) => tail(args).await?,
    }
    Ok(())
}

// TODO: generate random unicode emoji's too to test monospace graphemes
fn babble(args: BabbleArgs) -> Result<()> {
    use log::{log, Level, LevelFilter};
    env_logger::builder().filter_level(LevelFilter::Trace).init();
    let text = include_str!("./babble.txt");
    let re = Regex::new(r#"([.!?"]\s+|\n\n)"#)?;
    let sentences = re
        .split(text)
        .map(|s| s.trim().replace("\n", " "))
        .filter(|s| s.len() > 10)
        .take(args.lines);
    let mut rng = rand::thread_rng();
    for sentence in sentences {
        let level = match rng.gen_range(0..=5) {
            0 => None,
            1 => Some(Level::Error),
            2 => Some(Level::Warn),
            3 => Some(Level::Info),
            4 => Some(Level::Debug),
            _ => Some(Level::Trace),
        };
        match level {
            None => println!("{}", sentence),
            Some(level) => log!(level, "{}", sentence),
        }
    }
    Ok(())
}

async fn server(args: ServerArgs) -> Result<()> {
    env_logger::init();
    let routes = warp::any().and(warp::ws()).map(|ws: warp::ws::Ws| {
        ws.on_upgrade(|ws| async move {
            if let Err(e) = connection::handle_ws(ws).await {
                error!("while handling websocket connection: {}", e);
            }
        })
    });
    warp::serve(routes).run(args.bind).await;
    Ok(())
}

async fn tail(args: TailArgs) -> Result<()> {
    env_logger::init();
    let filter = args.filter.as_ref().map(|s| Regex::new(s)).transpose()?;
    let (mut ctx, mut rx_tail) =
        connection::Context::new(args.log_file.clone(), args.cols, filter)?;
    loop {
        rx_tail.changed().await?;
        match { *rx_tail.borrow_and_update() } {
            Some(len) => {
                let inc = ctx.read_to(len).await?;
                for line in inc {
                    println!("{}", serde_json::to_string(&line)?);
                }
            }
            None => {
                println!("file lost");
                return Ok(());
            }
        }
    }
}
