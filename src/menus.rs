use crate::{
    model::{Session, Story},
    save::{SlotId, SlotInfo, SlotState},
    settings::Settings,
};
use eframe::egui::{self, Color32, RichText};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SlotMode {
    Save,
    Load,
}

pub enum MenuAction {
    Close,
    Slot(SlotId),
    Confirm(SlotId),
    Cancel,
    ImportLegacy,
}

pub fn history(ctx: &egui::Context, story: &Story, session: &Session) -> bool {
    let mut open = true;
    egui::Window::new("历史记录")
        .id(egui::Id::new("history_window"))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(720.0)
        .resizable(true)
        .show(ctx, |ui| {
            ui.label(format!(
                "最近 {} 条（最多保留 1000 条，不支持回滚）",
                session.history.len()
            ));
            egui::ScrollArea::vertical()
                .id_salt("history_entries")
                .max_height(470.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for (index, entry) in session.history.iter().enumerate() {
                        let frame = &story.frames[&entry.frame_id];
                        let name = frame
                            .speaker
                            .as_ref()
                            .and_then(|id| story.characters.get(id))
                            .map(String::as_str)
                            .unwrap_or("旁白");
                        ui.label(
                            RichText::new(format!("{} · {}  /  {}", index + 1, name, frame.id))
                                .color(Color32::from_rgb(240, 202, 137)),
                        );
                        ui.label(RichText::new(&frame.text).size(19.0));
                        if let Some(choice) = &entry.choice_id {
                            let option = frame
                                .options
                                .iter()
                                .find(|option| &option.id == choice)
                                .expect("validated history option");
                            ui.label(
                                RichText::new(format!("选择：{}", option.text))
                                    .color(Color32::LIGHT_BLUE),
                            );
                        }
                        ui.separator();
                    }
                });
        });
    open
}

fn age(timestamp: u64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let minutes = now.saturating_sub(timestamp) / 60_000;
    match minutes {
        0 => "刚刚".into(),
        1..60 => format!("{minutes} 分钟前"),
        60..1440 => format!("{} 小时前", minutes / 60),
        _ => format!("{} 天前", minutes / 1440),
    }
}

pub fn slots(
    ctx: &egui::Context,
    infos: &[SlotInfo],
    mode: SlotMode,
    pending: Option<SlotId>,
    legacy: bool,
) -> Option<MenuAction> {
    let mut open = true;
    let mut action = None;
    let title = if mode == SlotMode::Save {
        "保存 · 手动槽位"
    } else {
        "读档 · 手动槽位"
    };
    egui::Window::new(title)
        .id(egui::Id::new("slots_window"))
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(720.0)
        .resizable(false)
        .show(ctx, |ui| {
            if let Some(slot) = pending {
                ui.colored_label(
                    Color32::YELLOW,
                    format!("确认覆盖槽位 {}？原存档将被替换。", slot.number()),
                );
                ui.horizontal(|ui| {
                    if ui.button("确认覆盖").clicked() {
                        action = Some(MenuAction::Confirm(slot));
                    }
                    if ui.button("取消").clicked() {
                        action = Some(MenuAction::Cancel);
                    }
                });
                ui.separator();
            }
            egui::ScrollArea::vertical()
                .id_salt("save_slots")
                .max_height(425.0)
                .show(ui, |ui| {
                    for info in infos {
                        ui.group(|ui| {
                            ui.set_min_width(670.0);
                            ui.horizontal(|ui| {
                                ui.strong(format!("槽位 {}", info.slot.number()));
                                let label = if mode == SlotMode::Save {
                                    "保存到此槽"
                                } else {
                                    "读取此槽"
                                };
                                let enabled = pending.is_none()
                                    && (mode == SlotMode::Save
                                        || matches!(info.state, SlotState::Ready { .. }));
                                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                                    action = Some(MenuAction::Slot(info.slot));
                                }
                            });
                            match &info.state {
                                SlotState::Empty => {
                                    ui.label("空槽");
                                }
                                SlotState::Ready {
                                    frame_id,
                                    preview,
                                    saved_at_unix_ms,
                                } => {
                                    ui.label(format!(
                                        "{}  ·  {}",
                                        age(*saved_at_unix_ms),
                                        frame_id
                                    ));
                                    let preview: String = preview.chars().take(70).collect();
                                    ui.label(preview);
                                }
                                SlotState::Invalid(error) => {
                                    ui.colored_label(
                                        Color32::LIGHT_RED,
                                        format!("损坏或不兼容：{error}"),
                                    );
                                }
                            }
                        });
                    }
                });
            if mode == SlotMode::Load
                && legacy
                && ui
                    .add_enabled(
                        pending.is_none(),
                        egui::Button::new("导入 v0.1 存档（旧档不含历史）"),
                    )
                    .clicked()
            {
                action = Some(MenuAction::ImportLegacy);
            }
        });
    if !open {
        action = Some(MenuAction::Close);
    }
    action
}

pub fn settings(ctx: &egui::Context, settings: &Settings, audio_status: &str) -> (Settings, bool) {
    let mut open = true;
    let mut edited = settings.clone();
    let mut close = false;
    egui::Window::new("设置")
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(500.0)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("文字速度（0 为整句显示）");
            ui.add(
                egui::Slider::new(&mut edited.text_speed, 0.0..=80.0)
                    .integer()
                    .suffix(" 字素簇/秒"),
            );
            ui.separator();
            ui.checkbox(&mut edited.muted, "总静音");
            ui.add(egui::Slider::new(&mut edited.bgm_volume, 0.0..=1.0).text("背景音乐"));
            ui.add(egui::Slider::new(&mut edited.voice_volume, 0.0..=1.0).text("语音"));
            ui.add(egui::Slider::new(&mut edited.se_volume, 0.0..=1.0).text("音效"));
            ui.separator();
            ui.label(audio_status);
            if ui.button("保存设置并返回").clicked() {
                close = true;
            }
        });
    (edited, open && !close)
}
