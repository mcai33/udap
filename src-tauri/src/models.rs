use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeInfo {
    pub id: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial_number: Option<String>,
    pub interface: Option<u8>,
    pub is_hid_interface: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetInfo {
    pub name: String,
    pub family: String,
    pub architecture: String,
    pub aliases: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectRequest {
    pub probe_id: String,
    pub protocol: String,
    pub speed_khz: u32,
    pub connect_under_reset: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectResult {
    pub target: TargetInfo,
    pub actual_speed_khz: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashRequest {
    pub probe_id: String,
    pub target_name: String,
    pub firmware_path: String,
    pub protocol: String,
    pub speed_khz: u32,
    pub connect_under_reset: bool,
    pub base_address: Option<u64>,
    pub verify: bool,
    pub chip_erase: bool,
    pub reset_after: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashEvent {
    pub stage: &'static str,
    pub completed: u64,
    pub total: Option<u64>,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlashResult {
    pub target_name: String,
    pub elapsed_ms: u128,
}
