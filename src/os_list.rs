use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsList {
    pub imager: ImagerInfo,
    pub os_list: Vec<OsListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImagerInfo {
    pub latest_version: String,
    pub url: String,
    pub devices: Vec<Device>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub name: String,
    pub tags: Vec<String>,
    pub icon: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub matching_type: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsListItem {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub random: bool,

    // Subitems (for categories)
    #[serde(default)]
    pub subitems: Vec<OsListItem>,

    // Image specific fields
    pub url: Option<String>,
    pub extract_size: Option<u64>,
    pub extract_sha256: Option<String>,
    pub image_download_size: Option<u64>,
    pub image_download_sha256: Option<String>,
    pub release_date: Option<String>,
    pub init_format: Option<String>,

    #[serde(default)]
    pub devices: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,

    pub website: Option<String>,
    pub tooltip: Option<String>,
    pub architecture: Option<String>,
    #[serde(default, rename = "enable_rpi_connect")]
    pub enable_rpi_connect: bool,
}
