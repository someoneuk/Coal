use eframe::egui::Color32;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Ok,
    Warn,
    Error,
}

impl Level {
    pub fn color(self) -> Color32 {
        match self {
            Level::Info => Color32::from_rgb(130, 170, 220),
            Level::Ok => Color32::from_rgb(110, 220, 150),
            Level::Warn => Color32::from_rgb(235, 190, 90),
            Level::Error => Color32::from_rgb(240, 110, 110),
        }
    }

    pub fn tag(self) -> &'static str {
        match self {
            Level::Info => "INFO",
            Level::Ok => " OK ",
            Level::Warn => "WARN",
            Level::Error => "ERR ",
        }
    }
}

#[derive(Clone)]
pub struct Entry {
    pub time: String,
    pub level: Level,
    pub message: String,
}

impl Entry {
    pub fn new(level: Level, message: impl Into<String>) -> Self {
        Self {
            time: clock(),
            level,
            message: message.into(),
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
