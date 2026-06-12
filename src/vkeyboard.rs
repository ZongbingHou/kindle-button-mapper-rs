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

fn supported_keys() -> Vec<Key> {
    let mut v = vec![];
    // Letters
    for k in [
        Key::KEY_A, Key::KEY_B, Key::KEY_C, Key::KEY_D, Key::KEY_E, Key::KEY_F,
        Key::KEY_G, Key::KEY_H, Key::KEY_I, Key::KEY_J, Key::KEY_K, Key::KEY_L,
        Key::KEY_M, Key::KEY_N, Key::KEY_O, Key::KEY_P, Key::KEY_Q, Key::KEY_R,
        Key::KEY_S, Key::KEY_T, Key::KEY_U, Key::KEY_V, Key::KEY_W, Key::KEY_X,
        Key::KEY_Y, Key::KEY_Z,
    ] { v.push(k); }
    // Digits + function + navigation + misc
    for k in [
        Key::KEY_0, Key::KEY_1, Key::KEY_2, Key::KEY_3, Key::KEY_4,
        Key::KEY_5, Key::KEY_6, Key::KEY_7, Key::KEY_8, Key::KEY_9,
        Key::KEY_F1, Key::KEY_F2, Key::KEY_F3, Key::KEY_F4, Key::KEY_F5,
        Key::KEY_F6, Key::KEY_F7, Key::KEY_F8, Key::KEY_F9, Key::KEY_F10,
        Key::KEY_F11, Key::KEY_F12,
        Key::KEY_UP, Key::KEY_DOWN, Key::KEY_LEFT, Key::KEY_RIGHT,
        Key::KEY_HOME, Key::KEY_END, Key::KEY_PAGEUP, Key::KEY_PAGEDOWN,
        Key::KEY_INSERT, Key::KEY_DELETE, Key::KEY_BACKSPACE,
        Key::KEY_ENTER, Key::KEY_SPACE, Key::KEY_TAB, Key::KEY_ESC,
        Key::KEY_LEFTSHIFT, Key::KEY_RIGHTSHIFT,
        Key::KEY_LEFTCTRL, Key::KEY_RIGHTCTRL,
        Key::KEY_LEFTALT, Key::KEY_RIGHTALT,
        Key::KEY_LEFTMETA, Key::KEY_RIGHTMETA,
        Key::KEY_CAPSLOCK, Key::KEY_NUMLOCK,
        Key::KEY_MINUS, Key::KEY_EQUAL, Key::KEY_LEFTBRACE, Key::KEY_RIGHTBRACE,
        Key::KEY_SEMICOLON, Key::KEY_APOSTROPHE, Key::KEY_GRAVE,
        Key::KEY_BACKSLASH, Key::KEY_COMMA, Key::KEY_DOT, Key::KEY_SLASH,
        Key::KEY_VOLUMEUP, Key::KEY_VOLUMEDOWN, Key::KEY_MUTE,
        Key::KEY_POWER, Key::KEY_SLEEP, Key::KEY_WAKEUP,
        Key::KEY_MENU, Key::KEY_BACK, Key::KEY_HOMEPAGE,
        Key::KEY_BRIGHTNESSUP, Key::KEY_BRIGHTNESSDOWN,
    ] { v.push(k); }
    v
}
