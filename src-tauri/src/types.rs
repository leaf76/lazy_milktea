use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub brand: String,
    pub model: String,
    pub android_version: String,
    pub api_level: i32,
    pub build_id: String,
    pub fingerprint: String,
    pub uptime_ms: i64,
    pub report_time: String,
    pub battery: Option<BatteryInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatteryInfo {
    pub level: i32,
    pub temp_c: f32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEvent {
    pub ts: String,
    pub kind: String,
    pub pid: Option<i32>,
    pub process: Option<String>,
    pub tid: Option<i32>,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRow {
    pub ts: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts_iso: Option<String>,
    pub level: String,
    pub tag: String,
    pub pid: i32,
    pub tid: i32,
    pub msg: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, Hash)]
#[serde(rename_all = "camelCase")]
pub struct LogFilters {
    pub ts_from: Option<String>,
    pub ts_to: Option<String>,
    pub levels: Option<Vec<String>>, // ["E","F"] etc.
    pub tag: Option<String>,
    pub pid: Option<i32>,
    pub tid: Option<i32>,
    pub text: Option<String>,
    pub not_text: Option<String>,
    pub text_mode: Option<String>,      // "plain" | "regex"
    pub case_sensitive: Option<bool>,
}
