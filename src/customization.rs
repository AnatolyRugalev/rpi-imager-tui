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

impl CustomizationOptions {
    pub fn config_path() -> Option<std::path::PathBuf> {
        if let Ok(home) = std::env::var("HOME") {
            let path = std::path::Path::new(&home).join(".config/rpi-imager-tui/config.json");
            Some(path)
        } else {
            None
        }
    }

    pub fn load() -> Self {
        if let Some(path) = Self::config_path() {
            if path.exists() {
                if let Ok(file) = std::fs::File::open(path) {
                    if let Ok(opts) = serde_json::from_reader(file) {
                        return opts;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(file) = std::fs::File::create(path) {
                let _ = serde_json::to_writer_pretty(file, self);
            }
        }
    }

    pub fn needs_customization(&self) -> bool {
        // Check if any option is non-default
        self.hostname != "raspberrypi"
            || self.ssh_enabled
            || !self.wifi_ssid.is_empty()
            || self.user_name != "pi"
            || self.password.is_some()
            || self.timezone != "Europe/London"
            || self.keyboard_layout != "gb"
            || self.locale != "en_GB.UTF-8"
    }

    pub fn generate_firstrun_script(&self) -> String {
        let mut script = String::from("#!/bin/bash\n");

        // Better safety
        script.push_str("set -e\n");

        // Wait for system to settle slightly?
        // script.push_str("sleep 5\n");

        // 1. User Account
        // Modern RPi OS might not have 'pi' user by default.
        // We attempt to create the user if it doesn't exist, or update password if it does.
        let user = &self.user_name;
        let pwd = self.password.as_deref().unwrap_or("");

        if !user.is_empty() && !pwd.is_empty() {
            // Check if user exists
            script.push_str(&format!("if id \"{}\" &>/dev/null; then\n", user));
            // User exists, update password
            script.push_str(&format!(
                "    echo \"{}:{}\" | chpasswd\n",
                user,
                shell_escape(pwd) // chpasswd takes plaintext on stdin usually
            ));
            script.push_str("else\n");
            // Create user
            script.push_str(&format!("    useradd -m -s /bin/bash \"{}\"\n", user));
            script.push_str(&format!(
                "    echo \"{}:{}\" | chpasswd\n",
                user,
                shell_escape(pwd)
            ));
            // Add to sudoers/groups
            script.push_str(&format!(
                "    usermod -aG sudo,video,audio,gpio,plugdev,netdev,dialout,users \"{}\"\n",
                user
            ));
            script.push_str("fi\n");
        }

        // 2. Hostname
        if !self.hostname.is_empty() {
            script.push_str(&format!(
                "CURRENT_HOSTNAME=$(cat /etc/hostname)\n\
                 if [ \"$CURRENT_HOSTNAME\" != \"{}\" ]; then\n\
                     echo \"{}\" > /etc/hostname\n\
                     sed -i \"s/127.0.1.1.*$CURRENT_HOSTNAME/127.0.1.1\\t{}/g\" /etc/hosts\n\
                     hostnamectl set-hostname \"{}\"\n\
                 fi\n",
                self.hostname, self.hostname, self.hostname, self.hostname
            ));
        }

        // 3. Locale / Timezone / Keyboard
        if self.timezone != "Europe/London" {
            script.push_str(&format!("timedatectl set-timezone \"{}\"\n", self.timezone));
            script.push_str(&format!(
                "rm /etc/localtime\n\
                  echo \"{}\" > /etc/timezone\n\
                  dpkg-reconfigure -f noninteractive tzdata\n",
                self.timezone
            ));
        }

        // Keyboard layout is tricky in headless, usually handled by /etc/default/keyboard
        if self.keyboard_layout != "gb" {
            script.push_str(&format!(
                "sed -i 's/XKBLAYOUT=\".*\"/XKBLAYOUT=\"{}\"/' /etc/default/keyboard\n\
                  setupcon || true\n",
                self.keyboard_layout
            ));
        }

        // Locale generation
        // e.g. en_US.UTF-8
        if self.locale != "en_GB.UTF-8" {
            // Uncomment the locale in /etc/locale.gen
            script.push_str(&format!(
                "sed -i 's/^# *{} /{} /' /etc/locale.gen\n",
                regex_escape(&self.locale),
                self.locale
            ));
            script.push_str("locale-gen\n");
            script.push_str(&format!("update-locale LANG={}\n", self.locale));
        }

        // 4. SSH
        if self.ssh_enabled {
            script.push_str("systemctl enable ssh\n");
            script.push_str("systemctl start ssh\n");

            if !self.ssh_password_auth {
                // Disable password auth
                script.push_str("sed -i 's/#PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config\n");
                script.push_str("sed -i 's/PasswordAuthentication yes/PasswordAuthentication no/' /etc/ssh/sshd_config\n");
            }

            if !self.ssh_public_keys.is_empty() {
                let home_dir = if user == "root" {
                    "/root".to_string()
                } else {
                    format!("/home/{}", user)
                };
                script.push_str(&format!("mkdir -p {}/.ssh\n", home_dir));
                script.push_str(&format!(
                    "echo \"{}\" >> {}/.ssh/authorized_keys\n",
                    self.ssh_public_keys, home_dir
                ));
                script.push_str(&format!("chown -R {}:{} {}/.ssh\n", user, user, home_dir));
                script.push_str(&format!("chmod 700 {}/.ssh\n", home_dir));
                script.push_str(&format!("chmod 600 {}/.ssh/authorized_keys\n", home_dir));
            }
        }

        // 5. WiFi
        // We write wpa_supplicant.conf
        if !self.wifi_ssid.is_empty() {
            let scan_ssid = if self.wifi_hidden { "scan_ssid=1" } else { "" };
            script.push_str(&format!(
                "cat > /etc/wpa_supplicant/wpa_supplicant.conf <<EOF\n\
                ctrl_interface=DIR=/var/run/wpa_supplicant GROUP=netdev\n\
                update_config=1\n\
                country={}\n\
                network={{\n\
                    ssid=\"{}\"\n\
                    psk=\"{}\"\n\
                    {}\n\
                }}\nEOF\n",
                self.wifi_country, self.wifi_ssid, self.wifi_password, scan_ssid
            ));
            // Ensure permissions
            script.push_str("chmod 600 /etc/wpa_supplicant/wpa_supplicant.conf\n");
            script.push_str("rfkill unblock wifi || true\n");
            // Restart networking might be needed, but usually reboot handles it.
        }

        // Clean up self
        // script.push_str("rm -f /boot/firstrun.sh\n");
        // Remove the cmdline patch? That's harder from within.
        // RPi Imager's method (systemd.run) is ephemeral if we only append to cmdline for one boot?
        // Actually systemd.run in cmdline persists until cmdline is edited back.
        // But usually we just leave it or let the user fix it.
        // Ideally, we would revert cmdline.txt here.
        script.push_str("sed -i 's/ systemd.run=\\/boot\\/firstrun.sh//g' /boot/cmdline.txt\n");
        script.push_str("sed -i 's/ systemd.run_success_action=reboot//g' /boot/cmdline.txt\n");
        script
            .push_str("sed -i 's/ systemd.unit=kernel-command-line.target//g' /boot/cmdline.txt\n");

        script
    }
}

fn shell_escape(s: &str) -> String {
    s.replace("\"", "\\\"").replace("$", "\\$")
}

fn regex_escape(s: &str) -> String {
    s.replace(".", "\\.")
}
