use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::Path;
use std::process::Command;
use crate::customization::CustomizationOptions;

pub fn apply_customization(device_path: &str, options: &CustomizationOptions) -> Result<()> {
    if !options.needs_customization() {
        return Ok(());
    }

    let boot_partition = get_boot_partition(device_path);
    let mount_point = format!("/tmp/rpi-imager-tui-mnt-{}", std::process::id());

    // Ensure directory exists
    fs::create_dir_all(&mount_point).context("Failed to create temp mount point")?;

    // Wait a moment for kernel to refresh partition table after write
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Refresh partition table just in case
    let _ = Command::new("partprobe").arg(device_path).output();
    std::thread::sleep(std::time::Duration::from_secs(1));

    // Mount
    // We try to mount with full permissions
    let status = Command::new("mount")
        .arg(&boot_partition)
        .arg(&mount_point)
        .status()
        .context(format!("Failed to mount boot partition {}", boot_partition))?;

    if !status.success() {
        return Err(anyhow!("Failed to mount boot partition. Exit code: {:?}", status.code()));
    }

    // Use a closure to ensure unmount happens on error
    let result = (|| -> Result<()> {
        // 1. Write firstrun.sh
        let script_content = options.generate_firstrun_script();
        let script_path = Path::new(&mount_point).join("firstrun.sh");
        fs::write(&script_path, script_content).context("Failed to write firstrun.sh")?;

        // Make executable (chmod +x) - though FAT doesn't store permissions, it helps if it's ext4
        let _ = Command::new("chmod").arg("+x").arg(script_path.to_str().unwrap()).status();

        // 2. Modify cmdline.txt
        let cmdline_path = Path::new(&mount_point).join("cmdline.txt");
        if cmdline_path.exists() {
            let mut cmdline = fs::read_to_string(&cmdline_path).context("Failed to read cmdline.txt")?;

            // Remove old entries if any (sanity check)
            cmdline = cmdline.replace(" systemd.run=/boot/firstrun.sh", "");
            cmdline = cmdline.replace(" systemd.run_success_action=reboot", "");
            cmdline = cmdline.replace(" systemd.unit=kernel-command-line.target", "");

            // Append new ones
            // Ensure we append to the single line, space separated
            let trimmed = cmdline.trim();
            let new_cmdline = format!(
                "{} systemd.run=/boot/firstrun.sh systemd.run_success_action=reboot systemd.unit=kernel-command-line.target",
                trimmed
            );

            fs::write(&cmdline_path, new_cmdline).context("Failed to update cmdline.txt")?;
        } else {
             // If cmdline.txt doesn't exist, this might not be RPi OS or partition structure is different.
             // We warn but continue.
             eprintln!("Warning: cmdline.txt not found in boot partition.");
        }

        // 3. Optional: config.txt
        // (Not currently implemented in CustomizationOptions, but placeholder for future)

        Ok(())
    })();

    // Unmount
    let umount_status = Command::new("umount")
        .arg(&mount_point)
        .status()
        .context("Failed to unmount boot partition")?;

    // Cleanup
    let _ = fs::remove_dir(&mount_point);

    if !umount_status.success() {
        return Err(anyhow!("Failed to unmount. Check if busy."));
    }

    result
}

fn get_boot_partition(device_path: &str) -> String {
    // Heuristic for partition name
    if device_path.chars().last().unwrap().is_numeric() {
        format!("{}p1", device_path)
    } else {
        format!("{}1", device_path)
    }
}
