use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};

use eframe::egui::{self, Color32, RichText};

use crate::inj::{find_process, inject_dll};
use crate::log::{Entry, Level};

const MAX_LOG: usize = 400;

pub struct App {
    dll_path: String,
    process_name: String,
    detected_pid: Option<u32>,
    detected_name: Option<String>,
    entries: Vec<Entry>,
    tx: Sender<Entry>,
    rx: Receiver<Entry>,
    busy: bool,
}

impl App {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            dll_path: String::new(),
            process_name: "RobloxPlayerBeta".into(),
            detected_pid: None,
            detected_name: None,
            entries: Vec::new(),
            tx,
            rx,
            busy: false,
        };
        app.push(Level::Info, "Launcher ready.");
        app.refresh();
        app
    }

    fn push(&mut self, level: Level, message: impl Into<String>) {
        self.entries.push(Entry::new(level, message));
        if self.entries.len() > MAX_LOG {
            self.entries.remove(0);
        }
    }

    fn refresh(&mut self) {
        match find_process(&self.process_name) {
            Some(info) => {
                self.detected_pid = Some(info.pid);
                self.detected_name = Some(info.name.clone());
                self.push(
                    Level::Ok,
                    format!("Target found: {} (PID {})", info.name, info.pid),
                );
            }
            None => {
                self.detected_pid = None;
                self.detected_name = None;
                self.push(
                    Level::Warn,
                    format!("No live process matching '{}'", self.process_name),
                );
            }
        }
    }

    fn browse(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Select DLL")
            .add_filter("Dynamic-link library", &["dll"])
            .pick_file()
        {
            self.dll_path = path.to_string_lossy().into_owned();
            self.push(Level::Info, format!("Selected: {}", self.dll_path));
        }
    }

    fn inject(&mut self) {
        if self.busy {
            return;
        }
        let (pid, name) = match (self.detected_pid, self.detected_name.clone()) {
            (Some(pid), Some(name)) => (pid, name),
            _ => {
                self.push(Level::Error, "No target process detected. Hit Refresh.");
                return;
            }
        };
        if self.dll_path.trim().is_empty() {
            self.push(Level::Error, "Choose a DLL first.");
            return;
        }
        if !Path::new(&self.dll_path).is_file() {
            self.push(Level::Error, "DLL path does not point to a file.");
            return;
        }

        self.busy = true;
        let tx = self.tx.clone();
        let dll = self.dll_path.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Entry::new(
                Level::Info,
                format!("Injecting into {name} (PID {pid})..."),
            ));
            match inject_dll(pid, Path::new(&dll)) {
                Ok(result) => {
                    let _ = tx.send(Entry::new(
                        Level::Ok,
                        format!(
                            "Injected into PID {}. Remote module base 0x{:X}.",
                            result.pid, result.module_base
                        ),
                    ));
                }
                Err(err) => {
                    let _ = tx.send(Entry::new(Level::Error, format!("Injection failed: {err}")));
                }
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        while let Ok(entry) = self.rx.try_recv() {
            self.push(entry.level, entry.message);
        }

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("Executor")
                    .size(24.0)
                    .color(Color32::from_rgb(235, 240, 250)),
            );
            ui.label(
                RichText::new("internal · rust")
                    .size(12.0)
                    .color(Color32::from_gray(130)),
            );
        });
        ui.add_space(2.0);
        ui.separator();
        ui.add_space(6.0);

        // Target panel
        ui.group(|ui| {
            ui.label(RichText::new("Target").size(13.0).strong());
            ui.horizontal(|ui| {
                ui.label("Process:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.process_name)
                        .desired_width(260.0)
                        .hint_text("RobloxPlayerBeta"),
                );
                if ui.button("Refresh").clicked() {
                    self.refresh();
                }
            });
            ui.add_space(4.0);
            if let (Some(pid), Some(name)) = (&self.detected_pid, &self.detected_name) {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        Color32::from_rgb(110, 220, 150),
                        RichText::new(format!("●  RUNNING   {name}  ·  PID {pid}")).monospace(),
                    );
                });
            } else {
                ui.colored_label(
                    Color32::from_rgb(235, 190, 90),
                    RichText::new("○  NOT RUNNING").monospace(),
                );
            }
        });

        ui.add_space(8.0);

        // Payload panel
        ui.group(|ui| {
            ui.label(RichText::new("Payload").size(13.0).strong());
            ui.horizontal(|ui| {
                ui.label("DLL:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.dll_path)
                        .desired_width(430.0)
                        .hint_text("path to runtime.dll"),
                );
                if ui.button("Browse...").clicked() {
                    self.browse();
                }
            });
        });

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            let button = egui::Button::new(
                RichText::new(if self.busy {
                    "  Injecting...  "
                } else {
                    "  Inject  "
                })
                .size(15.0)
                .strong(),
            );
            if ui.add_enabled(!self.busy, button).clicked() {
                self.inject();
            }
            ui.label(
                RichText::new("LoadLibrary remote-thread · x64")
                    .size(11.0)
                    .color(Color32::from_gray(120)),
            );
        });

        ui.add_space(10.0);
        ui.separator();
        ui.label(RichText::new("Log").size(13.0).strong());
        ui.add_space(2.0);

        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for entry in &self.entries {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(&entry.time)
                                .color(Color32::from_gray(110))
                                .monospace(),
                        );
                        ui.label(
                            RichText::new(entry.level.tag())
                                .color(entry.level.color())
                                .monospace()
                                .strong(),
                        );
                        ui.label(RichText::new(&entry.message).color(Color32::from_gray(215)));
                    });
                }
            });
    }
}
