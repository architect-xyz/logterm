use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcQuery<T> {
    pub id: u64,
    pub method: String,
    pub params: T,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcResponse<T> {
    pub id: u64,
    pub result: Option<T>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}
