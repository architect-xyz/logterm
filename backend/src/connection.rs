use crate::{
    json_rpc,
    parser::{self, DisplayLine},
};
use anyhow::Result;
use futures_util::{select_biased, stream::SplitSink, FutureExt, SinkExt, StreamExt};
use log::{debug, error};
use notify::{event::EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{future, path::PathBuf};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt, SeekFrom},
    sync::watch,
};
use warp::ws::{Message, WebSocket};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogsRequest {
    pub cols: usize,
    pub filter: Option<String>,
    pub log_file: PathBuf,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogsTail {
    pub display_lines: Vec<DisplayLine>,
}

#[derive(Debug)]
pub struct Context {
    cols: usize,
    filter: Option<Regex>,
    file: PathBuf,
    _watcher: RecommendedWatcher,
    pos: u64,
    lines_read: usize,
}

impl Context {
    pub fn new(
        file: PathBuf,
        cols: usize,
        filter: Option<Regex>,
    ) -> Result<(Self, watch::Receiver<Option<u64>>)> {
        let len = std::fs::metadata(&file)?.len();
        let (tx, rx) = watch::channel(None);
        tx.send_replace(Some(len));
        let mut watcher = notify::recommended_watcher({
            let file = file.clone();
            move |res: Result<notify::Event, notify::Error>| {
                use notify::event::ModifyKind;
                match res {
                    Ok(ev) => match ev.kind {
                        EventKind::Modify(ModifyKind::Data(_)) => {
                            debug!("watched file {} data changed", file.display());
                            if let Ok(meta) = std::fs::metadata(&file) {
                                tx.send_replace(Some(meta.len()));
                            }
                        }
                        EventKind::Modify(ModifyKind::Name(_)) => {
                            debug!("watched file {} was renamed", file.display());
                        }
                        EventKind::Remove(_) => {
                            debug!("watched file {} was removed", file.display());
                        }
                        _ => {}
                    },
                    Err(e) => {
                        error!("watch error: {e}");
                    }
                }
            }
        })?;
        watcher.watch(&file, RecursiveMode::NonRecursive)?;
        Ok((Self { cols, filter, file, _watcher: watcher, pos: 0, lines_read: 0 }, rx))
    }

    /// Returns the incremental read
    pub async fn read_to(&mut self, len: u64) -> Result<Vec<DisplayLine>> {
        if self.pos >= len {
            // CR alee: handle non-appends
            return Ok(vec![]);
        }
        let mut lines = vec![];
        let mut file = File::open(&self.file).await?;
        file.seek(SeekFrom::Start(self.pos)).await?;
        let mut contents = String::new();
        file.read_to_string(&mut contents).await?;
        // iterate over complete lines only (ending \r\n or \n)
        for line in contents.split_inclusive("\n") {
            if !line.ends_with("\n") {
                break;
            }
            let line = line.trim_end_matches("\n");
            let line = line.trim_end_matches("\r");
            if let Ok(Some(p)) = parser::parse_log_line(
                self.lines_read,
                self.cols,
                line,
                self.filter.as_ref(),
            ) {
                lines.extend(p);
            }
            self.pos += line.len() as u64;
            self.lines_read += 1;
        }
        Ok(lines)
    }
}

async fn herald_of_the_change(
    ctx: &mut Option<(Context, watch::Receiver<Option<u64>>)>,
) -> Result<(&mut Context, Option<u64>)> {
    if let Some((ref mut ctx, rx)) = ctx.as_mut() {
        rx.changed().await?;
        let changed = { *rx.borrow_and_update() };
        Ok((ctx, changed))
    } else {
        future::pending().await
    }
}

async fn handle_ws_message(
    tx: &mut SplitSink<WebSocket, Message>,
    ctx: &mut Option<(Context, watch::Receiver<Option<u64>>)>,
    msg: Message,
) -> Result<()> {
    if let Ok(s) = msg.to_str() {
        debug!("received: {}", s);
        let q: json_rpc::Request<LogsRequest> = serde_json::from_str(s)?;
        let filter = q.params.filter.as_ref().map(|s| Regex::new(s)).transpose()?;
        *ctx = Some(Context::new(q.params.log_file, q.params.cols, filter)?);
        tx.send(Message::text(serde_json::to_string(&json_rpc::Response {
            id: q.id,
            result: Some(()),
            error: None,
        })?))
        .await?;
    }
    Ok(())
}

async fn handle_changed(
    tx: &mut SplitSink<WebSocket, Message>,
    ctx: &mut Context,
    changed: Option<u64>,
) -> Result<()> {
    match changed {
        Some(len) => {
            let inc = ctx.read_to(len).await?;
            tx.send(Message::text(serde_json::to_string(&json_rpc::Notification {
                method: json_rpc::Method::Tail,
                params: LogsTail { display_lines: inc },
            })?))
            .await?;
        }
        None => {
            // file closed
            tx.send(Message::text(serde_json::to_string(&json_rpc::Notification {
                method: json_rpc::Method::Done,
                params: (),
            })?))
            .await?;
        }
    }
    Ok(())
}

pub async fn handle_ws(ws: WebSocket) -> Result<()> {
    let (mut tx, mut rx) = ws.split();
    let mut ctx: Option<(Context, watch::Receiver<Option<u64>>)> = None;
    loop {
        select_biased! {
            msg = rx.next().fuse() => {
                if let Some(msg) = msg {
                    let msg = msg?;
                    handle_ws_message(&mut tx, &mut ctx, msg).await?;
                }
            }
            r = herald_of_the_change(&mut ctx).fuse() => {
                debug!("changed");
                let (ctx, rx_tail) = r?;
                handle_changed(&mut tx, ctx, rx_tail).await?;
            }
        }
    }
}
