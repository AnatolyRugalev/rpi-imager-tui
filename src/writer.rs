use crate::drivelist::Drive;
use crate::os_list::OsListItem;
use crate::{AppMessage, WritingPhase};
use anyhow::{Context, Result, anyhow};
use async_compression::tokio::bufread::{GzipDecoder, XzDecoder, ZstdDecoder};
use futures::TryStreamExt;
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::io::SeekFrom;
use std::time::Instant;
use tokio::fs::OpenOptions;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::mpsc;
use tokio_util::io::StreamReader;

pub async fn write_image(os: OsListItem, drive: Drive, tx: mpsc::Sender<AppMessage>) -> Result<()> {
    let url = os
        .url
        .as_deref()
        .ok_or_else(|| anyhow!("No URL provided for the selected OS"))?;

    let extract_size = os.extract_size.unwrap_or(0);
    let extract_sha256 = os.extract_sha256.as_deref();

    // Send 0% progress
    let _ = tx.send(AppMessage::WriteProgress(0.0)).await;
    let _ = tx
        .send(AppMessage::WritingPhase(WritingPhase::Writing))
        .await;
    let _ = tx
        .send(AppMessage::WriteStatus("Starting download...".to_string()))
        .await;

    // Start Download
    let client = Client::builder()
        .user_agent("rpi-imager-tui/0.1")
        .build()
        .unwrap_or_else(|_| Client::new());

    let res = client
        .get(url)
        .send()
        .await
        .context(format!("Failed to download from {}", url))?;

    if !res.status().is_success() {
        return Err(anyhow!("Download failed with status: {}", res.status()));
    }

    // Convert reqwest stream to AsyncRead
    let stream = res
        .bytes_stream()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e));
    let stream_reader = StreamReader::new(stream);
    let buf_reader = BufReader::with_capacity(1024 * 1024, stream_reader);

    let url_parsed = reqwest::Url::parse(url)
        .unwrap_or_else(|_| reqwest::Url::parse(&format!("http://dummy/{}", url)).unwrap());
    let path = url_parsed.path();

    // Determine compression type from URL and setup decoder
    let mut decoder: Box<dyn AsyncRead + Unpin + Send> = if path.ends_with(".xz") {
        Box::new(XzDecoder::new(buf_reader))
    } else if path.ends_with(".gz") {
        Box::new(GzipDecoder::new(buf_reader))
    } else if path.ends_with(".zst") {
        Box::new(ZstdDecoder::new(buf_reader))
    } else if path.ends_with(".zip") {
        return Err(anyhow!(
            "ZIP files are not supported yet. Please choose an .xz, .gz, or .zst image."
        ));
    } else {
        // Assume uncompressed if no known extension match
        Box::new(buf_reader)
    };

    // Open target device for writing
    let device_file = OpenOptions::new()
        .write(true)
        .read(true)
        .open(&drive.name)
        .await
        .context(format!(
            "Failed to open device {}. Ensure you are running with root privileges (sudo).",
            drive.name
        ))?;

    // 4MB Buffer
    let mut buffer = vec![0u8; 4 * 1024 * 1024];
    let mut total_written = 0u64;
    let mut hasher = Sha256::new();

    // Wrap device_file in BufWriter for better write performance (4MB buffer)
    let mut buf_writer = BufWriter::with_capacity(4 * 1024 * 1024, device_file);

    let start_time = Instant::now();
    let mut last_update = Instant::now();

    loop {
        let n = decoder
            .read(&mut buffer)
            .await
            .context("Failed to read/decompress image stream")?;

        if n == 0 {
            break;
        }

        buf_writer
            .write_all(&buffer[..n])
            .await
            .context("Failed to write to storage device")?;

        // Update checksum
        hasher.update(&buffer[..n]);

        total_written += n as u64;

        // Update progress every 500ms
        if last_update.elapsed().as_millis() > 500 {
            let elapsed_secs = start_time.elapsed().as_secs_f64();
            let speed_mb_s = if elapsed_secs > 0.0 {
                (total_written as f64 / 1024.0 / 1024.0) / elapsed_secs
            } else {
                0.0
            };

            if extract_size > 0 {
                let progress = (total_written as f64 / extract_size as f64) * 100.0;
                // Clamp to 99% until synced and verified
                let display_progress = if progress > 99.0 { 99.0 } else { progress };
                let _ = tx.send(AppMessage::WriteProgress(display_progress)).await;
                let _ = tx
                    .send(AppMessage::WriteStatus(format!(
                        "Writing... {:.1}% ({:.1} MB/s)",
                        display_progress, speed_mb_s
                    )))
                    .await;
            } else {
                let _ = tx
                    .send(AppMessage::WriteStatus(format!(
                        "Writing... {} MB ({:.1} MB/s)",
                        total_written / 1024 / 1024,
                        speed_mb_s
                    )))
                    .await;
            }
            last_update = Instant::now();
        }
    }

    // Flush buffer
    buf_writer
        .flush()
        .await
        .context("Failed to flush write buffer")?;

    let _ = tx
        .send(AppMessage::WriteStatus("Syncing to disk...".to_string()))
        .await;

    // Retrieve underlying file to sync and seek
    let mut device_file = buf_writer.into_inner();

    // Ensure all data is physically written to disk
    device_file
        .sync_all()
        .await
        .context("Failed to sync data to device")?;

    let _ = tx
        .send(AppMessage::WritingPhase(WritingPhase::Verifying))
        .await;

    let _ = tx
        .send(AppMessage::WriteStatus("Verifying download...".to_string()))
        .await;

    // Calculate source hash
    let source_hash = hasher.finalize();
    let source_hash_hex = hex::encode(source_hash);

    // Verify download integrity if expected hash is provided
    if let Some(expected_hash) = extract_sha256 {
        if source_hash_hex.to_lowercase() != expected_hash.to_lowercase() {
            return Err(anyhow!(
                "Download verification failed!\nExpected: {}\nCalculated: {}",
                expected_hash,
                source_hash_hex
            ));
        }
    }

    let _ = tx
        .send(AppMessage::WriteStatus(
            "Verifying write (reading back)...".to_string(),
        ))
        .await;

    // Verify write integrity by reading back from device
    device_file
        .seek(SeekFrom::Start(0))
        .await
        .context("Failed to seek to start of device for verification")?;

    let mut verify_hasher = Sha256::new();
    let mut total_read = 0u64;
    let start_time = Instant::now();
    let mut last_update = Instant::now();

    loop {
        let remaining = total_written - total_read;
        if remaining == 0 {
            break;
        }

        let to_read = std::cmp::min(buffer.len() as u64, remaining) as usize;
        let n = device_file
            .read(&mut buffer[..to_read])
            .await
            .context("Failed to read from device for verification")?;

        if n == 0 {
            return Err(anyhow!("Unexpected EOF during verification"));
        }

        verify_hasher.update(&buffer[..n]);
        total_read += n as u64;

        if last_update.elapsed().as_millis() > 500 {
            let elapsed_secs = start_time.elapsed().as_secs_f64();
            let speed_mb_s = if elapsed_secs > 0.0 {
                (total_read as f64 / 1024.0 / 1024.0) / elapsed_secs
            } else {
                0.0
            };

            if extract_size > 0 {
                let progress = (total_read as f64 / extract_size as f64) * 100.0;
                let _ = tx.send(AppMessage::VerifyProgress(progress)).await;
                let _ = tx
                    .send(AppMessage::WriteStatus(format!(
                        "Verifying... {:.1}% ({:.1} MB/s)",
                        progress, speed_mb_s
                    )))
                    .await;
            }
            last_update = Instant::now();
        }
    }

    let on_disk_hash_hex = hex::encode(verify_hasher.finalize());

    if on_disk_hash_hex != source_hash_hex {
        return Err(anyhow!(
            "Write verification failed!\nSource hash: {}\nOn-disk hash: {}",
            source_hash_hex,
            on_disk_hash_hex
        ));
    }

    // Send completion
    let _ = tx.send(AppMessage::WriteFinished).await;

    Ok(())
}
