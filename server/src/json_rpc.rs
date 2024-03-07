// CR alee: this is a bastardized incomplete impl of the nominal JSON-RPC
// spec but I don't care until there's an actual need for interop

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// Request to list all available logsets
    List,
    /// Request to change which logset to display and tail
    Logs,
    /// Notification from the server, additional display lines
    Tail,
    /// Notification form the server that the logset has fused,
    /// or no more tailing is possible
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestHeader {
    pub id: u64,
    pub method: Method,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request<T> {
    pub id: u64,
    pub method: Method,
    pub params: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification<T> {
    pub method: Method,
    pub params: T,
}

#[derive(Debug, Clone, Serialize)]
pub struct Response<T> {
    pub id: u64,
    pub result: Option<T>,
    pub error: Option<Error>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: i32,
    pub message: String,
}
