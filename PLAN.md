
# Implementation Plan - RPi Imager TUI

## Goal
Replicate Raspberry Pi Imager 2.0 functionality as a Terminal User Interface (TUI) application using Rust.

## Tech Stack
- **Language**: Rust
- **TUI Framework**: `ratatui` + `crossterm`
- **Async Runtime**: `tokio`
- **HTTP Client**: `reqwest`
- **Serialization**: `serde`, `serde_json`
- **Decompression**: `async-compression` (xz, gzip, zstd)
- **Privilege Management**: Check for root/sudo (required for block device access)

## Phase 1: Project Setup & OS List Navigation [COMPLETED]
- [x] Initialize Rust Project.
- [x] Define data structures for `OsList` mirroring `os_list_imagingutility_v4.json`.
- [x] Implement OS selection UI with drill-down navigation and breadcrumbs.
- [x] Prioritize loading local JSON file for offline development.

## Phase 2: Storage Device Detection [COMPLETED]
- [x] Implement `lsblk` parsing to detect block devices.
- [x] Filter out system drives to prevent accidental overwrites.
- [x] Create UI for selecting target storage.
- [x] Implement `--debug` flag to create and list a fake loopback device for testing safely.

## Phase 3: Writing & Verification [COMPLETED]
- [x] Implement streaming download with `reqwest`.
- [x] Implement streaming decompression (`.xz`, `.gz`, `.zst`).
- [x] Implement writing to block device.
- [x] Implement verification:
    - [x] Verify download SHA256 against JSON metadata.
    - [x] Verify written data by reading back from disk.
- [x] Real-time progress bar in UI.

## Phase 4: UI Refinement [COMPLETED]
- [x] Update layout to match RPi Imager 2.0 (Sidebar steps).
- [x] Add "Device Selection" step at the beginning.
- [x] Add footer with:
    - [x] Description/Tooltip for the selected item.
    - [x] Context-aware key bindings.
- [x] Improve navigation flow (Esc behavior, Back logic).

## Phase 5: Image Customization (Advanced Options) [TODO]
**Goal**: Generate cloud-init/firstrun scripts effectively porting `CustomisationGenerator.cpp`.

1.  **Configuration State**:
    - Struct to hold: Hostname, SSH enable/password/key, WiFi SSID/Password, Locale.
2.  **UI - Form**:
    - Popup/Modal or separate screen with input fields for these options.
    - Toggle for "Apply Customization".
3.  **Generator Logic**:
    - Implement logic to generate `user-data` (cloud-init) or `firstrun.sh`.
    - Inject these files into the FAT partition of the target image after writing (requires mounting or manipulating FAT filesystem).

## Phase 6: Final Polish [TODO]
1.  **Telemetry**: Implement anonymous usage stats (opt-out).
2.  **CLI Args**: Support passing image path directly (local file support).
3.  **Error Handling**: Improve error messages and recovery states.
4.  **Binaries**: Create release builds for Linux (amd64, arm64).

## References
- **Original Source**: `rpi-imager/src/`
    - `imagewriter.cpp`: Main logic.
    - `oslistmodel.cpp`: JSON parsing.
    - `customization_generator.cpp`: Config generation.
- **JSON URL**: `https://downloads.raspberrypi.com/os_list_imagingutility_v4.json`
