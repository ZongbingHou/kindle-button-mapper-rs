use evdev::uinput::{VirtualDevice, VirtualDeviceBuilder};
use evdev::{AttributeSet, Key};
use log::{info, warn};
use std::fs;
use std::path::Path;
use std::process::Command;

const UINPUT_DEV: &str = "/dev/uinput";
const TARGET_FILE: &str = "/var/run/kindle-button-mapper-key-target";

pub fn try_init() -> Option<VirtualDevice> {
    ensure_uinput_node().ok()?;

    let mut keys = AttributeSet::<Key>::new();
    for k in supported_keys() {
        keys.insert(k);
    }

    let dev = match VirtualDeviceBuilder::new()
        .and_then(|b| b.name(b"kindle-button-mapper").with_keys(&keys))
        .and_then(|b| b.build())
    {
        Ok(d) => d,
        Err(e) => {
            warn!("uinput device create failed: {} — keyboard mappings will not inject events", e);
            return None;
        }
    };

    let mut device = dev;
    if let Ok(mut paths) = device.enumerate_dev_nodes_blocking() {
        if let Some(Ok(path)) = paths.next() {
            let s = path.display().to_string();
            if let Err(e) = fs::write(TARGET_FILE, &s) {
                warn!("Cannot write {}: {}", TARGET_FILE, e);
            } else {
                info!("Virtual keyboard at {} (target written to {})", s, TARGET_FILE);
            }
        }
    }
    Some(device)
}

fn ensure_uinput_node() -> Result<(), String> {
    if Path::new(UINPUT_DEV).exists() {
        return Ok(());
    }
    // Kernel built with CONFIG_INPUT_UINPUT=y but no devtmpfs node — create it.
    let status = Command::new("mknod")
        .args([UINPUT_DEV, "c", "10", "223"])
        .status()
        .map_err(|e| format!("mknod missing: {}", e))?;
    if !status.success() {
        return Err(format!("mknod exit {}", status.code().unwrap_or(-1)));
    }
    let _ = Command::new("chmod").args(["600", UINPUT_DEV]).status();
    Ok(())
}

fn supported_keys() -> impl Iterator<Item = Key> {
    // All KEY_* codes. The BTN_* ranges (0x100-0x15f mouse/gamepad,
    // 0x2c0+ trigger-happy) are skipped so the device enumerates as a
    // plain keyboard.
    (1..0x100).chain(0x160..0x2c0).map(Key::new)
}
