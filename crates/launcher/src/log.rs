use std::time::{SystemTime, UNIX_EPOCH};

pub struct LogMsg {
    pub time: String,
    pub tag: &'static str,
    pub text: String,
}

impl LogMsg {
    pub fn new(tag: &'static str, text: impl Into<String>) -> Self {
        Self {
            time: clock(),
            tag,
            text: text.into(),
        }
    }
}

fn clock() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!(
        "{:02}:{:02}:{:02}",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60
    )
}
