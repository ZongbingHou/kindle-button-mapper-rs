use configparser::ini::Ini;
use evdev::Key;
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

/// D-pad directions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DpadDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Trigger buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Trigger {
    LT, // Left Trigger (Brake, code 10)
    RT, // Right Trigger (Gas, code 9)
}

#[derive(Debug, Clone)]
pub struct DeviceConfig {
    pub id: String,
    pub name: Option<String>,
    pub path: Option<String>,
    pub uniq: Option<String>,
    pub grab: bool,
    pub mappings: HashMap<Key, String>,
    pub long_press_mappings: HashMap<Key, String>,
    pub dpad_mappings: HashMap<DpadDirection, String>,
    pub dpad_longpress_mappings: HashMap<DpadDirection, String>,
    pub trigger_mappings: HashMap<Trigger, String>,
    pub trigger_longpress_mappings: HashMap<Trigger, String>,
}

impl DeviceConfig {
    fn new(id: String) -> Self {
        Self {
            id,
            name: None,
            path: None,
            uniq: None,
            grab: true,
            mappings: HashMap::new(),
            long_press_mappings: HashMap::new(),
            dpad_mappings: HashMap::new(),
            dpad_longpress_mappings: HashMap::new(),
            trigger_mappings: HashMap::new(),
            trigger_longpress_mappings: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct Config {
    pub devices: Vec<DeviceConfig>,
    pub debounce_ms: u64,
    pub long_press_ms: u64,
    pub repeat_ms: u64,
    pub log_buttons: bool,
    pub on_connect: Option<String>,
    pub on_disconnect: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            devices: Vec::new(),
            debounce_ms: 200,
            long_press_ms: 500,
            repeat_ms: 100,
            log_buttons: false,
            on_connect: None,
            on_disconnect: None,
        }
    }
}

const DEFAULT_CONFIG: &str = "# Kindle Button Mapper - default config
# Edit via the Button Mapper WAF app, or by hand.

[settings]
debounce_ms = 0
log_buttons = true
long_press_ms = 500
repeat_ms = 100
";

impl Config {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let p = path.as_ref();
        if !p.exists() {
            std::fs::write(p, DEFAULT_CONFIG)
                .map_err(|e| format!("Cannot create {}: {}", p.display(), e))?;
        }

        let mut ini = Ini::new();
        ini.load(p).map_err(|e| format!("Failed to load config: {}", e))?;

        let mut config = Config::default();
        let map = ini.get_map_ref();

        // [settings] — global daemon settings
        if let Some(settings) = map.get("settings") {
            if let Some(Some(v)) = settings.get("debounce_ms") {
                config.debounce_ms = v.parse().unwrap_or(config.debounce_ms);
            }
            if let Some(Some(v)) = settings.get("long_press_ms") {
                config.long_press_ms = v.parse().unwrap_or(config.long_press_ms);
            }
            if let Some(Some(v)) = settings.get("repeat_ms") {
                config.repeat_ms = v.parse().unwrap_or(config.repeat_ms);
            }
            if let Some(Some(v)) = settings.get("log_buttons") {
                config.log_buttons = matches!(v.to_lowercase().as_str(), "true" | "yes" | "1");
            }
            if let Some(Some(v)) = settings.get("on_connect") {
                if !v.is_empty() {
                    config.on_connect = Some(v.clone());
                }
            }
            if let Some(Some(v)) = settings.get("on_disconnect") {
                if !v.is_empty() {
                    config.on_disconnect = Some(v.clone());
                }
            }
        }

        // First pass: collect device IDs from [device.NAME] sections, ordered by appearance.
        let mut devices: HashMap<String, DeviceConfig> = HashMap::new();
        let mut order: Vec<String> = Vec::new();

        for section in map.keys() {
            let parts: Vec<&str> = section.split('.').collect();
            if parts.first() != Some(&"device") || parts.len() < 2 {
                continue;
            }
            let id = parts[1].to_string();
            if !devices.contains_key(&id) {
                devices.insert(id.clone(), DeviceConfig::new(id.clone()));
                order.push(id);
            }
        }

        // Second pass: fill in each device's fields.
        for section in map.keys() {
            let parts: Vec<&str> = section.split('.').collect();
            if parts.first() != Some(&"device") {
                continue;
            }
            let id = match parts.get(1) {
                Some(s) => s.to_string(),
                None => continue,
            };
            let dev = match devices.get_mut(&id) {
                Some(d) => d,
                None => continue,
            };
            let entries = match map.get(section) {
                Some(e) => e,
                None => continue,
            };

            match parts.get(2).copied() {
                None => {
                    // [device.NAME] — device meta
                    if let Some(Some(v)) = entries.get("name") {
                        dev.name = Some(v.clone());
                    }
                    if let Some(Some(v)) = entries.get("path") {
                        dev.path = Some(v.clone());
                    }
                    if let Some(Some(v)) = entries.get("uniq") {
                        dev.uniq = Some(v.clone());
                    }
                    if let Some(Some(v)) = entries.get("grab") {
                        dev.grab = matches!(v.to_lowercase().as_str(), "true" | "yes" | "1");
                    }
                }
                Some("buttons") => fill_key_map(entries, &mut dev.mappings),
                Some("longpress") => fill_key_map(entries, &mut dev.long_press_mappings),
                Some("dpad") => fill_dpad_map(entries, &mut dev.dpad_mappings),
                Some("dpad_longpress") => fill_dpad_map(entries, &mut dev.dpad_longpress_mappings),
                Some("triggers") => fill_trigger_map(entries, &mut dev.trigger_mappings),
                Some("triggers_longpress") => {
                    fill_trigger_map(entries, &mut dev.trigger_longpress_mappings)
                }
                _ => {}
            }
        }

        // Backward compat: flat [device] + [buttons] / [dpad] / [triggers] etc.
        // Promote to a single device named "default".
        if let Some(legacy) = map.get("device") {
            // Only if the legacy section actually has device fields (path/name)
            // *and* we didn't already see a [device.NAME] section.
            let has_legacy_fields = legacy.contains_key("path")
                || legacy.contains_key("name")
                || legacy.contains_key("uniq")
                || legacy.contains_key("grab");
            if has_legacy_fields && devices.is_empty() {
                let mut dev = DeviceConfig::new("default".to_string());
                if let Some(Some(v)) = legacy.get("name") {
                    dev.name = Some(v.clone());
                }
                if let Some(Some(v)) = legacy.get("path") {
                    dev.path = Some(v.clone());
                }
                if let Some(Some(v)) = legacy.get("uniq") {
                    dev.uniq = Some(v.clone());
                }
                if let Some(Some(v)) = legacy.get("grab") {
                    dev.grab = matches!(v.to_lowercase().as_str(), "true" | "yes" | "1");
                }
                if let Some(e) = map.get("buttons") {
                    fill_key_map(e, &mut dev.mappings);
                }
                if let Some(e) = map.get("longpress") {
                    fill_key_map(e, &mut dev.long_press_mappings);
                }
                if let Some(e) = map.get("dpad") {
                    fill_dpad_map(e, &mut dev.dpad_mappings);
                }
                if let Some(e) = map.get("dpad_longpress") {
                    fill_dpad_map(e, &mut dev.dpad_longpress_mappings);
                }
                if let Some(e) = map.get("triggers") {
                    fill_trigger_map(e, &mut dev.trigger_mappings);
                }
                if let Some(e) = map.get("triggers_longpress") {
                    fill_trigger_map(e, &mut dev.trigger_longpress_mappings);
                }
                devices.insert("default".to_string(), dev);
                order.push("default".to_string());
            }
        }

        for id in order {
            if let Some(dev) = devices.remove(&id) {
                if dev.path.is_some() || dev.name.is_some() || dev.uniq.is_some() {
                    config.devices.push(dev);
                }
            }
        }

        // Empty device list is allowed — daemon will idle and let the
        // WAF add devices before a restart.
        Ok(config)
    }
}

fn fill_key_map(
    entries: &HashMap<String, Option<String>>,
    out: &mut HashMap<Key, String>,
) {
    for (k, v) in entries {
        if let (Some(key), Some(script)) = (parse_key(k), v) {
            out.insert(key, script.clone());
        }
    }
}

fn fill_dpad_map(
    entries: &HashMap<String, Option<String>>,
    out: &mut HashMap<DpadDirection, String>,
) {
    for (k, v) in entries {
        if let (Some(dir), Some(script)) = (parse_dpad_direction(k), v) {
            out.insert(dir, script.clone());
        }
    }
}

fn fill_trigger_map(
    entries: &HashMap<String, Option<String>>,
    out: &mut HashMap<Trigger, String>,
) {
    for (k, v) in entries {
        if let (Some(t), Some(script)) = (parse_trigger(k), v) {
            out.insert(t, script.clone());
        }
    }
}

fn parse_key(s: &str) -> Option<Key> {
    // Try parsing as decimal
    if let Ok(code) = s.parse::<u16>() {
        return Some(Key::new(code));
    }

    // Try parsing as hex (0x prefix)
    if let Some(hex) = s.strip_prefix("0x") {
        if let Ok(code) = u16::from_str_radix(hex, 16) {
            return Some(Key::new(code));
        }
    }

    // Try parsing as named key (evdev knows every KEY_* name)
    Key::from_str(&s.to_uppercase()).ok()
}

fn parse_dpad_direction(s: &str) -> Option<DpadDirection> {
    match s.to_lowercase().as_str() {
        "up" => Some(DpadDirection::Up),
        "down" => Some(DpadDirection::Down),
        "left" => Some(DpadDirection::Left),
        "right" => Some(DpadDirection::Right),
        _ => None,
    }
}

fn parse_trigger(s: &str) -> Option<Trigger> {
    match s.to_lowercase().as_str() {
        "lt" | "left" => Some(Trigger::LT),
        "rt" | "right" => Some(Trigger::RT),
        _ => None,
    }
}
