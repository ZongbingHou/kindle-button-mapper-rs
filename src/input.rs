use evdev::Device;
use log::{debug, info, warn};
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

const INPUT_DIR: &str = "/dev/input";

pub struct InputHandler {
    device_name: Option<String>,
    device_path: Option<String>,
    device_uniq: Option<String>,
    grab: bool,
}

impl InputHandler {
    pub fn new(
        device_name: Option<String>,
        device_path: Option<String>,
        device_uniq: Option<String>,
        grab: bool,
    ) -> Self {
        Self {
            device_name,
            device_path,
            device_uniq,
            grab,
        }
    }

    pub fn open(&self) -> Result<Device, String> {
        let has_identity = self.device_uniq.as_ref().is_some_and(|u| !u.is_empty())
            || self.device_name.as_ref().is_some_and(|n| !n.is_empty());

        if has_identity {
            // Try configured path first as a quick check
            if let Some(ref path_str) = self.device_path {
                let path = Path::new(path_str);
                if path.exists() {
                    if let Ok(dev) = Device::open(path) {
                        if self.matches_device(&dev) {
                            return self.finish_open(dev);
                        }
                        debug!("Device at {} doesn't match identity, scanning...", path.display());
                    }
                }
            }

            if let Some(dev) = self.scan_for_device()? {
                return self.finish_open(dev);
            }

            info!("Waiting for device to appear...");
            let dev = self.wait_for_matching_device()?;
            return self.finish_open(dev);
        }

        // No identity — fall back to path-only
        if let Some(ref path_str) = self.device_path {
            let path = Path::new(path_str);
            if path.exists() {
                let dev = Device::open(path)
                    .map_err(|e| format!("Cannot open {}: {}", path.display(), e))?;
                return self.finish_open(dev);
            }
            info!("Device {} not found, waiting...", path.display());
            self.wait_for_path(path)?;
            let dev = Device::open(path)
                .map_err(|e| format!("Cannot open {}: {}", path.display(), e))?;
            return self.finish_open(dev);
        }

        Err("No device name, uniq, or path specified".to_string())
    }

    fn matches_device(&self, dev: &Device) -> bool {
        if dev.name().unwrap_or("") == "kindle-button-mapper" {
            return false;
        }
        if let Some(ref uniq) = self.device_uniq {
            if !uniq.is_empty() && dev.unique_name().unwrap_or("") != uniq.as_str() {
                return false;
            }
        }
        if let Some(ref name) = self.device_name {
            if !name.is_empty() && dev.name().unwrap_or("") != name.as_str() {
                return false;
            }
        }
        true
    }

    fn scan_for_device(&self) -> Result<Option<Device>, String> {
        let entries = fs::read_dir(INPUT_DIR)
            .map_err(|e| format!("Cannot open {}: {}", INPUT_DIR, e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            let filename = path.file_name().and_then(OsStr::to_str).unwrap_or("");
            if !filename.starts_with("event") {
                continue;
            }
            match Device::open(&path) {
                Ok(dev) => {
                    debug!("Scanning {}: name={:?} uniq={:?}",
                        path.display(),
                        dev.name().unwrap_or(""),
                        dev.unique_name().unwrap_or(""));
                    if self.matches_device(&dev) {
                        info!("Found device at {}", path.display());
                        return Ok(Some(dev));
                    }
                }
                Err(e) => {
                    debug!("Cannot open {}: {}", path.display(), e);
                }
            }
        }
        Ok(None)
    }

    fn wait_for_matching_device(&self) -> Result<Device, String> {
        let inotify = Inotify::init(InitFlags::empty())
            .map_err(|e| format!("inotify_init failed: {}", e))?;
        inotify.add_watch(Path::new(INPUT_DIR), AddWatchFlags::IN_CREATE)
            .map_err(|e| format!("inotify_add_watch failed: {}", e))?;

        // A device that appeared between the caller's scan and the watch
        // being added would never produce an event — scan once more.
        if let Some(dev) = self.scan_for_device()? {
            return Ok(dev);
        }

        loop {
            let events = inotify.read_events()
                .map_err(|e| format!("inotify read failed: {}", e))?;

            for event in events {
                if let Some(event_name) = &event.name {
                    let name_str = event_name.to_string_lossy();
                    if !name_str.starts_with("event") {
                        continue;
                    }
                    let path = Path::new(INPUT_DIR).join(&*name_str);
                    thread::sleep(Duration::from_millis(100));

                    match Device::open(&path) {
                        Ok(dev) => {
                            if self.matches_device(&dev) {
                                info!("Found device at {}", path.display());
                                return Ok(dev);
                            }
                        }
                        Err(e) => {
                            debug!("Cannot open new device {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
    }

    fn wait_for_path(&self, target_path: &Path) -> Result<(), String> {
        let inotify = Inotify::init(InitFlags::empty())
            .map_err(|e| format!("inotify_init failed: {}", e))?;
        inotify.add_watch(Path::new(INPUT_DIR), AddWatchFlags::IN_CREATE)
            .map_err(|e| format!("inotify_add_watch failed: {}", e))?;

        let target_name = target_path.file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid device path")?;

        // Same race as above: the path may have appeared before the watch.
        if target_path.exists() {
            return Ok(());
        }

        loop {
            let events = inotify.read_events()
                .map_err(|e| format!("inotify read failed: {}", e))?;

            for event in events {
                if let Some(event_name) = &event.name {
                    if event_name.to_string_lossy() == target_name {
                        info!("Device {} appeared", target_path.display());
                        thread::sleep(Duration::from_millis(100));
                        return Ok(());
                    }
                }
            }
        }
    }

    fn finish_open(&self, mut device: Device) -> Result<Device, String> {
        if device.name().unwrap_or("") == "kindle-button-mapper" {
            return Err("Refusing to read our own virtual keyboard".to_string());
        }
        if self.grab {
            match device.grab() {
                Ok(()) => info!("Grabbed device exclusively"),
                Err(e) => warn!("Cannot grab device: {}, continuing without exclusive access", e),
            }
        } else {
            info!("Exclusive grab disabled, sharing device");
        }
        info!("Reading events from {} (uniq={:?})",
            device.name().unwrap_or("?"),
            device.unique_name().unwrap_or(""));
        Ok(device)
    }
}
