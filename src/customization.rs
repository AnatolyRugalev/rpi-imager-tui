use glob::glob;
use serde::{Deserialize, Serialize};
use std::io::BufRead;

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

        // Better safety (disable for some commands that might fail harmlessly)
        script.push_str("set +e\n");

        // 1. Hostname
        if !self.hostname.is_empty() {
            script.push_str(&format!(
                "CURRENT_HOSTNAME=$(cat /etc/hostname | tr -d \" \\t\\n\\r\")\n\
                 if [ -f /usr/lib/raspberrypi-sys-mods/imager_custom ]; then\n\
                     /usr/lib/raspberrypi-sys-mods/imager_custom set_hostname {}\n\
                 else\n\
                     echo {} > /etc/hostname\n\
                     sed -i \"s/127.0.1.1.*$CURRENT_HOSTNAME/127.0.1.1\\t{}/g\" /etc/hosts\n\
                 fi\n",
                shell_escape(&self.hostname),
                shell_escape(&self.hostname),
                self.hostname
            ));
        }

        // Determine first user (uid 1000) and home
        script.push_str("FIRSTUSER=$(getent passwd 1000 | cut -d: -f1)\n");
        script.push_str("FIRSTUSERHOME=$(getent passwd 1000 | cut -d: -f6)\n");

        // 2. SSH
        if self.ssh_enabled {
            if !self.ssh_public_keys.is_empty() {
                script.push_str("if [ -f /usr/lib/raspberrypi-sys-mods/imager_custom ]; then\n");
                script.push_str(&format!(
                    "   /usr/lib/raspberrypi-sys-mods/imager_custom enable_ssh -k '{}'\n",
                    self.ssh_public_keys
                ));
                script.push_str("else\n");
                script.push_str("   install -o \"$FIRSTUSER\" -m 700 -d \"$FIRSTUSERHOME/.ssh\"\n");
                script.push_str("   cat > \"$FIRSTUSERHOME/.ssh/authorized_keys\" <<'EOF'\n");
                script.push_str(&self.ssh_public_keys);
                script.push_str("\nEOF\n");
                script.push_str(
                    "   chown \"$FIRSTUSER:$FIRSTUSER\" \"$FIRSTUSERHOME/.ssh/authorized_keys\"\n",
                );
                script.push_str("   chmod 600 \"$FIRSTUSERHOME/.ssh/authorized_keys\"\n");

                if !self.ssh_password_auth {
                    script.push_str("   echo 'PasswordAuthentication no' >>/etc/ssh/sshd_config\n");
                }

                script.push_str("   systemctl enable ssh\n");
                script.push_str("fi\n");
            } else if self.ssh_password_auth {
                script.push_str("if [ -f /usr/lib/raspberrypi-sys-mods/imager_custom ]; then\n");
                script.push_str("   /usr/lib/raspberrypi-sys-mods/imager_custom enable_ssh\n");
                script.push_str("else\n");
                script.push_str("   systemctl enable ssh\n");
                script.push_str("fi\n");
            }
        }

        // 3. User Account

        let user = &self.user_name;

        let pwd = self.password.as_deref().unwrap_or("");

        if !user.is_empty() && !pwd.is_empty() {
            let pwd_hash = hash_password(pwd);

            script.push_str("if [ -f /usr/lib/userconf-pi/userconf ]; then\n");

            script.push_str(&format!(
                "   /usr/lib/userconf-pi/userconf {} {}\n",
                shell_escape(user),
                shell_escape(&pwd_hash)
            ));

            script.push_str("else\n");

            // Legacy/Manual fallback

            script.push_str(&format!(
                "   echo \"$FIRSTUSER:{}\" | chpasswd -e\n",
                shell_escape(&pwd_hash)
            ));

            script.push_str(&format!("   if [ \"$FIRSTUSER\" != \"{}\" ]; then\n", user));

            script.push_str(&format!("      usermod -l \"{}\" \"$FIRSTUSER\"\n", user));
            script.push_str(&format!(
                "      usermod -m -d \"/home/{}\" \"{}\"\n",
                user, user
            ));
            script.push_str(&format!("      groupmod -n \"{}\" \"$FIRSTUSER\"\n", user));

            // Fix autologin and sudoers
            script.push_str(
                "      if grep -q \"^autologin-user=\" /etc/lightdm/lightdm.conf ; then\n",
            );
            script.push_str(&format!(
                "         sed /etc/lightdm/lightdm.conf -i -e \"s/^autologin-user=.*/autologin-user={}/\"\n",
                user
            ));
            script.push_str("      fi\n");

            script.push_str(
                "      if [ -f /etc/systemd/system/getty@tty1.service.d/autologin.conf ]; then\n",
            );
            script.push_str(&format!(
                "         sed /etc/systemd/system/getty@tty1.service.d/autologin.conf -i -e \"s/$FIRSTUSER/{}/\"\n",
                user
            ));
            script.push_str("      fi\n");

            script.push_str("      if [ -f /etc/sudoers.d/010_pi-nopasswd ]; then\n");
            script.push_str(&format!(
                "         sed -i \"s/^$FIRSTUSER /{} /\" /etc/sudoers.d/010_pi-nopasswd\n",
                user
            ));
            script.push_str("      fi\n");
            script.push_str("   fi\n");
            script.push_str("fi\n");
        }

        // 4. WiFi
        if !self.wifi_ssid.is_empty() {
            let scan_ssid = if self.wifi_hidden { "scan_ssid=1" } else { "" };

            script.push_str("if [ -f /usr/lib/raspberrypi-sys-mods/imager_custom ]; then\n");
            let hidden_flag = if self.wifi_hidden { "-h" } else { "" };
            script.push_str(&format!(
                "   /usr/lib/raspberrypi-sys-mods/imager_custom set_wlan {} {} {} {}\n",
                hidden_flag,
                shell_escape(&self.wifi_ssid),
                shell_escape(&self.wifi_password),
                shell_escape(&self.wifi_country)
            ));
            script.push_str("else\n");

            script.push_str("cat >/etc/wpa_supplicant/wpa_supplicant.conf <<'WPAEOF'\n");
            if !self.wifi_country.is_empty() {
                script.push_str(&format!("country={}\n", self.wifi_country));
            }
            script.push_str("ctrl_interface=DIR=/var/run/wpa_supplicant GROUP=netdev\n");
            script.push_str("update_config=1\n");
            script.push_str("network={\n");
            script.push_str(&format!("    ssid=\"{}\"\n", self.wifi_ssid)); // Simple quoting for now
            script.push_str(&format!("    psk=\"{}\"\n", self.wifi_password));
            script.push_str(&format!("    {}\n", scan_ssid));
            script.push_str("}\n");
            script.push_str("WPAEOF\n");

            script.push_str("   chmod 600 /etc/wpa_supplicant/wpa_supplicant.conf\n");
            script.push_str("   rfkill unblock wifi || true\n");
            script.push_str("   for filename in /var/lib/systemd/rfkill/*:wlan ; do\n");
            script.push_str("       echo 0 > $filename\n");
            script.push_str("   done\n");
            script.push_str("fi\n");
        } else if !self.wifi_country.is_empty() {
            script.push_str("rfkill unblock wifi || true\n");
            script.push_str("for filename in /var/lib/systemd/rfkill/*:wlan ; do\n");
            script.push_str("  echo 0 > $filename\n");
            script.push_str("done\n");
        }

        // 5. Locale / Timezone / Keyboard
        if !self.keyboard_layout.is_empty() || !self.timezone.is_empty() || !self.locale.is_empty()
        {
            script.push_str("if [ -f /usr/lib/raspberrypi-sys-mods/imager_custom ]; then\n");
            if !self.keyboard_layout.is_empty() {
                script.push_str(&format!(
                    "   /usr/lib/raspberrypi-sys-mods/imager_custom set_keymap {}\n",
                    shell_escape(&self.keyboard_layout)
                ));
            }
            if !self.timezone.is_empty() {
                script.push_str(&format!(
                    "   /usr/lib/raspberrypi-sys-mods/imager_custom set_timezone {}\n",
                    shell_escape(&self.timezone)
                ));
            }
            script.push_str("else\n");

            // Fallback
            if !self.timezone.is_empty() {
                script.push_str("   rm -f /etc/localtime\n");
                script.push_str(&format!("   echo \"{}\" >/etc/timezone\n", self.timezone));
                script.push_str("   dpkg-reconfigure -f noninteractive tzdata\n");
            }

            if !self.keyboard_layout.is_empty() {
                script.push_str("cat >/etc/default/keyboard <<'KBEOF'\n");
                script.push_str("XKBMODEL=\"pc105\"\n");
                script.push_str(&format!("XKBLAYOUT=\"{}\"\n", self.keyboard_layout));
                script.push_str("XKBVARIANT=\"\"\n");
                script.push_str("XKBOPTIONS=\"\"\n");
                script.push_str("\n");
                script.push_str("KBEOF\n");
                script.push_str("   dpkg-reconfigure -f noninteractive keyboard-configuration\n");
            }

            // Locale generation (from previous implementation, compatible)
            if self.locale != "en_GB.UTF-8" {
                script.push_str(&format!(
                    "sed -i 's/^# *{} /{} /' /etc/locale.gen\n",
                    regex_escape(&self.locale),
                    self.locale
                ));
                script.push_str("locale-gen\n");
                script.push_str(&format!("update-locale LANG={}\n", self.locale));
            }

            script.push_str("fi\n");
        }

        // Cleanup
        script.push_str("rm -f /boot/firstrun.sh\n");
        script.push_str("sed -i 's| systemd.run.*||g' /boot/cmdline.txt\n");
        script.push_str("exit 0\n");

        script
    }
}

fn shell_escape(s: &str) -> String {
    s.replace("\"", "\\\"").replace("$", "\\$")
}

fn regex_escape(s: &str) -> String {
    s.replace(".", "\\.")
}

fn hash_password(password: &str) -> String {
    pwhash::sha512_crypt::hash(password).unwrap_or_else(|_| "".to_string())
}

pub fn discover_ssh_keys() -> Vec<String> {
    let mut keys = Vec::new();
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let ssh_dir = std::path::Path::new(&home).join(".ssh");

    // 1. Scan for .pub files
    let pattern = ssh_dir.join("*.pub");
    if let Some(pattern_str) = pattern.to_str() {
        if let Ok(paths) = glob(pattern_str) {
            for entry in paths.filter_map(Result::ok) {
                if let Ok(content) = std::fs::read_to_string(&entry) {
                    keys.push(content.trim().to_string());
                }
            }
        }
    }

    // 2. Read authorized_keys
    let auth_keys = ssh_dir.join("authorized_keys");
    if let Ok(file) = std::fs::File::open(auth_keys) {
        let reader = std::io::BufReader::new(file);
        for line in reader.lines().filter_map(Result::ok) {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                keys.push(trimmed.to_string());
            }
        }
    }

    // Deduplicate
    keys.sort();
    keys.dedup();
    keys
}
