use serde::Deserialize;
use std::error::Error;
use std::process::Command;

#[derive(Debug, Clone, Deserialize)]
struct LsblkOutput {
    blockdevices: Vec<LsblkDevice>,
}

#[derive(Debug, Clone, Deserialize)]
struct LsblkDevice {
    name: String,
    #[serde(deserialize_with = "parse_size")]
    size: u64,
    model: Option<String>,
    #[serde(rename = "type")]
    device_type: String,
    mountpoint: Option<String>,
    label: Option<String>,
    #[serde(default)]
    rm: Option<serde_json::Value>,
    #[serde(default)]
    ro: Option<serde_json::Value>,

    children: Option<Vec<LsblkDevice>>,
}

fn parse_size<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(deserializer)?;
    match v {
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| serde::de::Error::custom("Invalid size number")),
        serde_json::Value::String(s) => s.parse::<u64>().map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom("Invalid size format")),
    }
}

#[derive(Debug, Clone)]
pub struct Drive {
    pub name: String,        // e.g., /dev/sda
    pub description: String, // e.g., "Samsung SSD 860 (500 GB)"
    pub size: u64,
    pub removable: bool,
    pub readonly: bool,
    pub mountpoints: Vec<String>,
}

impl Drive {
    pub fn is_system(&self) -> bool {
        // Heuristic: if it contains root mountpoint "/", it is likely the system drive.
        self.mountpoints.iter().any(|mp| mp == "/")
    }
}

pub fn get_drives() -> Result<Vec<Drive>, Box<dyn Error>> {
    let debug = std::env::args().any(|arg| arg == "--debug");

    let output = Command::new("lsblk")
        .args(&[
            "-J",
            "-b",
            "-o",
            "NAME,SIZE,MODEL,TYPE,MOUNTPOINT,LABEL,RM,RO",
        ])
        .output()?;

    if !output.status.success() {
        return Err(format!("lsblk failed: {}", String::from_utf8_lossy(&output.stderr)).into());
    }

    let output_str = String::from_utf8(output.stdout)?;
    let lsblk_out: LsblkOutput = serde_json::from_str(&output_str)?;

    let mut drives = Vec::new();

    for device in lsblk_out.blockdevices {
        // We only care about physical disks, not partitions or loop devices at the top level
        if device.device_type != "disk" {
            continue;
        }

        let name = format!("/dev/{}", device.name);
        let size = device.size;
        let model = device
            .model
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());

        let removable = is_true(&device.rm);
        let readonly = is_true(&device.ro);

        // Collect mountpoints from device and children
        let mut mountpoints = Vec::new();
        if let Some(mp) = &device.mountpoint {
            mountpoints.push(mp.clone());
        }
        if let Some(children) = &device.children {
            collect_mountpoints(children, &mut mountpoints);
        }

        // Create a friendly description
        let description = if let Some(lbl) = &device.label {
            format!("{} - {} ({})", model, lbl, format_size(size))
        } else {
            format!("{} ({})", model, format_size(size))
        };

        drives.push(Drive {
            name,
            description,
            size,
            removable,
            readonly,
            mountpoints,
        });
    }

    if debug {
        let fake_path = "fake_sd_card.img";
        if !std::path::Path::new(fake_path).exists() {
            let f = std::fs::File::create(fake_path)?;
            f.set_len(4 * 1024 * 1024 * 1024)?; // 4 GB
        }

        drives.push(Drive {
            name: fake_path.to_string(),
            description: "Fake SD Card (Debug)".to_string(),
            size: 4 * 1024 * 1024 * 1024,
            removable: true,
            readonly: false,
            mountpoints: vec![],
        });
    }

    Ok(drives)
}

fn collect_mountpoints(devices: &[LsblkDevice], mountpoints: &mut Vec<String>) {
    for dev in devices {
        if let Some(mp) = &dev.mountpoint {
            mountpoints.push(mp.clone());
        }
        if let Some(children) = &dev.children {
            collect_mountpoints(children, mountpoints);
        }
    }
}

fn is_true(v: &Option<serde_json::Value>) -> bool {
    match v {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(s)) => s == "1" || s.to_lowercase() == "true",
        Some(serde_json::Value::Number(n)) => n.as_i64() == Some(1),
        _ => false,
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.0} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} B", bytes)
    }
}
