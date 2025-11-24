use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomizationOptions {
    pub hostname: String,
    pub timezone: String,
    pub keyboard_layout: String,

    // User settings
    pub user_name: String,
    pub password: Option<String>, // Plain text password, to be hashed later

    // SSH
    pub ssh_enabled: bool,
    pub ssh_password_auth: bool,
    pub ssh_public_keys: String,

    // WiFi
    pub wifi_ssid: String,
    pub wifi_password: String,
    pub wifi_country: String,
    pub wifi_hidden: bool,

    // Locale
    pub locale: String,

    // Options Tab
    pub telemetry: bool,
    pub eject_finished: bool,
}

impl Default for CustomizationOptions {
    fn default() -> Self {
        Self {
            hostname: "raspberrypi".to_string(),
            timezone: "Europe/London".to_string(),
            keyboard_layout: "gb".to_string(),
            user_name: "pi".to_string(),
            password: None,
            ssh_enabled: false,
            ssh_password_auth: true,
            ssh_public_keys: String::new(),
            wifi_ssid: String::new(),
            wifi_password: String::new(),
            wifi_country: "GB".to_string(),
            wifi_hidden: false,
            locale: "en_GB.UTF-8".to_string(),
            telemetry: true,
            eject_finished: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CustomizationTab {
    General,
    Services,
    Options,
}

impl CustomizationTab {
    pub fn next(&self) -> Self {
        match self {
            Self::General => Self::Services,
            Self::Services => Self::Options,
            Self::Options => Self::General,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            Self::General => Self::Options,
            Self::Services => Self::General,
            Self::Options => Self::Services,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Navigation,
    Editing,
}

pub struct CustomizationUiState {
    pub current_tab: CustomizationTab,
    pub selected_field_index: usize,
    pub input_mode: InputMode,
    // Temporary buffer for editing text fields
    pub input_buffer: String,
}

impl Default for CustomizationUiState {
    fn default() -> Self {
        Self {
            current_tab: CustomizationTab::General,
            selected_field_index: 0,
            input_mode: InputMode::Navigation,
            input_buffer: String::new(),
        }
    }
}

// Placeholder for future generator logic
impl CustomizationOptions {
    pub fn generate_firstrun_script(&self) -> String {
        let mut script = String::from("#!/bin/bash\n");

        // Hostname
        if !self.hostname.is_empty() {
            script.push_str(&format!(
                "echo {} > /etc/hostname\n",
                shell_quote(&self.hostname)
            ));
            script.push_str(&format!(
                "sed -i 's/127.0.1.1.*/127.0.1.1\\t{}/g' /etc/hosts\n",
                self.hostname
            ));
        }

        // SSH
        if self.ssh_enabled {
            script.push_str("systemctl enable ssh\n");
            script.push_str("systemctl start ssh\n");

            if !self.ssh_password_auth {
                script.push_str("echo 'PasswordAuthentication no' >> /etc/ssh/sshd_config\n");
            }

            if !self.ssh_public_keys.is_empty() {
                // Logic to add authorized_keys would go here
                // This is complex because we need to know the target user's home dir
                // For now, we'll assume standard pi user or whatever is created
            }
        }

        // WiFi (WPA Supplicant)
        if !self.wifi_ssid.is_empty() {
            script.push_str(&format!(
                "cat > /etc/wpa_supplicant/wpa_supplicant.conf <<EOF\n\
                ctrl_interface=DIR=/var/run/wpa_supplicant GROUP=netdev\n\
                update_config=1\n\
                country={}\n\
                network={{\n\
                    ssid=\"{}\"\n\
                    psk=\"{}\"\n",
                self.wifi_country, self.wifi_ssid, self.wifi_password
            ));

            if self.wifi_hidden {
                script.push_str("    scan_ssid=1\n");
            }

            script.push_str("}\nEOF\n");
            script.push_str("rfkill unblock wifi\n");
        }

        script
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace("'", "'\"'\"'"))
}
