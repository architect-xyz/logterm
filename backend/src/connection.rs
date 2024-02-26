use anyhow::Result;
use log::debug;
use warp::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use crate::{query, QueryArgs, QueryResponse};

pub async fn handle_ws(ws: WebSocket) -> Result<()> {
    use crate::json_rpc::*;
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
