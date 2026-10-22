use crate::{
    assets::{AssetCache, PreparedAssets},
    controller,
    loader::LoadedStory,
    model::{Session, Story},
    save::SaveStore,
    view::{self, UiIntent},
};
use eframe::egui;

pub struct PlayerApp {
    story: Story,
    session: Session,
    assets: AssetCache,
    saves: SaveStore,
    message: Option<String>,
}

impl PlayerApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        loaded: LoadedStory,
        prepared: PreparedAssets,
        saves: SaveStore,
    ) -> Self {
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        let assets = prepared.install(&cc.egui_ctx);
        let session = loaded.story.start();
        let message = if loaded.warnings.is_empty() {
            None
        } else {
            Some(loaded.warnings.join("\n"))
        };
        Self {
            story: loaded.story,
            session,
            assets,
            saves,
            message,
        }
    }

    fn handle(&mut self, intent: UiIntent) {
        let result = match intent {
            UiIntent::Play(intent) => controller::reduce(&self.story, &self.session, intent)
                .map(|session| self.session = session)
                .map_err(anyhow::Error::from),
            UiIntent::Save => self.saves.save(&self.story, &self.session).map(|()| {
                self.message = Some(format!("存档成功\n{}", self.saves.path().display()));
            }),
            UiIntent::Load => self.saves.load(&self.story).map(|session| {
                self.session = session;
                self.message = Some("读档成功".into());
            }),
            UiIntent::Restart => {
                self.session = self.story.start();
                Ok(())
            }
        };
        if let Err(error) = result {
            self.message = Some(format!("{error:#}"));
        }
    }
}

impl eframe::App for PlayerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut intent = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::BLACK))
            .show(ctx, |ui| {
                let frame = self.story.frame(&self.session).expect("validated session");
                intent = view::draw(ui, &self.story, frame, &self.assets, self.message.is_some());
            });
        if let Some(intent) = intent {
            self.handle(intent);
            ctx.request_repaint();
        }
        if let Some(message) = self.message.clone() {
            egui::Window::new("提示")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_max_width(600.0);
                    ui.label(message);
                    if ui.button("关闭").clicked() {
                        self.message = None;
                    }
                });
        }
    }
}
