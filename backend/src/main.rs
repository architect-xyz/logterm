use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use futures_util::{SinkExt, StreamExt};
use log::{debug, error};
use rand::Rng;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufRead, BufReader},
    net::SocketAddr,
    ops::Range,
    path::PathBuf,
};
use warp::{
    ws::{Message, WebSocket},
    Filter,
};

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
    /// Test query a log file
    Query(QueryArgs),
    /// Run the log server
    Server(ServerArgs),
}

#[derive(Args)]
struct BabbleArgs {
    #[arg(long, short_alias = 'n', default_value = "100")]
    lines: usize,
}

#[derive(Args, Debug, Clone, Deserialize)]
struct QueryArgs {
    #[arg(long)]
    cols: usize,
    #[arg(long)]
    filter: Option<String>,
    log_file: PathBuf,
    /// From display line number
    from: usize,
    /// To display line number; none for all
    to: Option<usize>,
}

#[derive(Args)]
struct ServerArgs {
    #[arg(long, default_value = "127.0.0.1:9000")]
    bind: SocketAddr,
}

#[derive(Debug, Clone, Serialize)]
struct QueryResponse {
    total_display_lines: usize,
    display_lines: Vec<parser::DisplayLine>,
    row_offset: usize,
}

impl QueryResponse {
    fn range(&self, range: Range<usize>) -> QueryResponse {
        let start = range.start.clamp(0, self.display_lines.len());
        let end = range.end.clamp(0, self.display_lines.len());
        let display_lines = self.display_lines[start..end].to_vec();
        QueryResponse {
            total_display_lines: self.total_display_lines,
            display_lines,
            row_offset: start,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Babble(args) => babble(args)?,
        Command::Query(args) => {
            let range_from = args.from;
            let range_to = args.to;
            let res = query(args)?;
            let range = range_from..range_to.unwrap_or(res.total_display_lines);
            for i in range {
                if let Some(line) = res.display_lines.get(i) {
                    println!("{}", serde_json::to_string_pretty(line)?);
                }
            }
        }
        Command::Server(args) => server(args).await?,
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

fn query(args: QueryArgs) -> Result<QueryResponse> {
    let filter = args.filter.as_ref().map(|s| Regex::new(s)).transpose()?;
    let file = File::open(&args.log_file)?;
    let reader = BufReader::new(file);
    let mut total_display_lines = 0;
    let mut display_lines = Vec::new();
    for (lln, line) in reader.lines().enumerate() {
        let line = line?;
        if let Some(parsed) =
            parser::parse_log_line(lln, args.cols, &line, filter.as_ref())?
        {
            total_display_lines += parsed.len();
            display_lines.extend(parsed);
        }
    }
    let res = QueryResponse { total_display_lines, display_lines, row_offset: 0 };
    Ok(res)
}

async fn server(args: ServerArgs) -> Result<()> {
    env_logger::init();
    let routes = warp::any().and(warp::ws()).map(|ws: warp::ws::Ws| {
        ws.on_upgrade(|ws| async move {
            if let Err(e) = handle_ws(ws).await {
                error!("while handling websocket connection: {}", e);
            }
        })
    });
    warp::serve(routes).run(args.bind).await;
    Ok(())
}

async fn handle_ws(ws: WebSocket) -> Result<()> {
    use json_rpc::*;
    let (mut tx, mut rx) = ws.split();
    let mut last: Option<(QueryArgs, QueryResponse)> = None;
    while let Some(Ok(msg)) = rx.next().await {
        if let Ok(s) = msg.to_str() {
            debug!("received: {}", s);
            let new_query: JsonRpcQuery<QueryArgs> = serde_json::from_str(s)?;
            // if query is substantially similar to the last one, use the cached response
            if let Some((last_query, last_response)) = last.as_ref() {
                if new_query.params.cols == last_query.cols
                    && new_query.params.filter == last_query.filter
                    && new_query.params.log_file == last_query.log_file
                {
                    let to =
                        new_query.params.to.unwrap_or(last_response.total_display_lines);
                    tx.send(Message::text(serde_json::to_string(&JsonRpcResponse {
                        id: new_query.id,
                        result: Some(last_response.range(new_query.params.from..to)),
                        error: None,
                    })?))
                    .await?;
                    continue;
                }
            }
            let range_from = new_query.params.from;
            let range_to = new_query.params.to;
            let response = query(new_query.params.clone())?;
            let range = range_from..range_to.unwrap_or(response.total_display_lines);
            last = Some((new_query.params, response.clone()));
            tx.send(Message::text(serde_json::to_string(&JsonRpcResponse {
                id: new_query.id,
                result: Some(response.range(range)),
                error: None,
            })?))
            .await?;
        }
    }
    Ok(())
}
