use std::fs;
use std::time::{Duration, SystemTime};

// Capture helper raises this; daemon drops its grab while it's fresh.
pub const PAUSE_FILE: &str = "/tmp/kindle-button-mapper-capture";
const FRESH: Duration = Duration::from_secs(20);

pub fn begin() -> std::io::Result<()> {
    fs::write(PAUSE_FILE, b"")
}

pub fn end() {
    let _ = fs::remove_file(PAUSE_FILE);
}

pub fn active() -> bool {
    let mtime = match fs::metadata(PAUSE_FILE).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(_) => return false,
    };
    match SystemTime::now().duration_since(mtime) {
        Ok(age) => age < FRESH,
        Err(_) => true,
    }
}
