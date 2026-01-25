use crate::customization::CustomizationOptions;
use crate::drivelist::Drive;
use crate::os_list::OsListItem;
use crate::{AppMessage, WritingPhase};
use base64::Engine;
use serde::{Deserialize, Serialize};
use std::process;
use tokio::sync::mpsc;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WorkerMessage {
    Progress(f64),
    VerifyProgress(f64),
    Status(String),
    Phase(String),
    Error(String),
    Finished,
}

pub async fn run_worker(args: Vec<String>) {
    // Parse arguments
    let mut image_url = String::new();
    let mut device_path = String::new();
    let mut sha256 = None;
    let mut size = None;
    let mut options_b64 = String::new();

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--image" => {
                i += 1;
                if i < args.len() {
                    image_url = args[i].clone();
                }
            }
            "--device" => {
                i += 1;
                if i < args.len() {
                    device_path = args[i].clone();
                }
            }
            "--sha256" => {
                i += 1;
                if i < args.len() {
                    sha256 = Some(args[i].clone());
                }
            }
            "--size" => {
                i += 1;
                if i < args.len() {
                    size = args[i].parse::<u64>().ok();
                }
            }
            "--options" => {
                i += 1;
                if i < args.len() {
                    options_b64 = args[i].clone();
                }
            }
            _ => {}
        }
        i += 1;
    }

    if image_url.is_empty() || device_path.is_empty() {
        eprintln!("Missing required arguments for worker");
        process::exit(1);
    }

    // Decode options
    let options: CustomizationOptions = if !options_b64.is_empty() {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(options_b64)
            .unwrap_or_default();
        serde_json::from_slice(&decoded).unwrap_or_default()
    } else {
        CustomizationOptions::default()
    };

    // Construct objects
    let os = OsListItem {
        name: "Worker Image".to_string(),
        url: Some(image_url),
        extract_sha256: sha256,
        extract_size: size,
        // Defaults
        description: String::new(),
        icon: None,
        random: false,
        subitems: Vec::new(),
        image_download_size: None,
        image_download_sha256: None,
        release_date: None,
        init_format: None,
        devices: Vec::new(),
        capabilities: Vec::new(),
        website: None,
        tooltip: None,
        architecture: None,
        enable_rpi_connect: false,
    };

    let drive = Drive {
        name: device_path,
        // Defaults
        description: "Target Drive".to_string(),
        size: 0,
        removable: true,
        readonly: false,
        mountpoints: Vec::new(),
    };

    let (tx, mut rx) = mpsc::channel::<AppMessage>(100);

    // Spawn writer
    tokio::spawn(async move {
        if let Err(e) = crate::writer::write_image(os, drive, options, tx.clone()).await {
            let _ = tx.send(AppMessage::WriteError(e.to_string())).await;
        }
    });

    // Loop and print JSON
    while let Some(msg) = rx.recv().await {
        let worker_msg = match msg {
            AppMessage::WriteProgress(p) => WorkerMessage::Progress(p),
            AppMessage::VerifyProgress(p) => WorkerMessage::VerifyProgress(p),
            AppMessage::WriteStatus(s) => WorkerMessage::Status(s),
            AppMessage::WritingPhase(p) => WorkerMessage::Phase(match p {
                WritingPhase::Writing => "Writing".to_string(),
                WritingPhase::Verifying => "Verifying".to_string(),
            }),
            AppMessage::WriteError(e) => WorkerMessage::Error(e),
            AppMessage::WriteFinished => WorkerMessage::Finished,
            AppMessage::OsListLoaded(_) => continue, // Should not happen
        };

        if let Ok(json) = serde_json::to_string(&worker_msg) {
            println!("{}", json);
        }

        if let WorkerMessage::Finished | WorkerMessage::Error(_) = worker_msg {
            break;
        }
    }
}
