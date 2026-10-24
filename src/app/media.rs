use super::PlayerApp;
use crate::{
    controller::{self, Intent},
    interface::host::Plan,
};
use anyhow::{Result, ensure};
impl PlayerApp {
    pub(super) fn advance_video(&mut self) -> Result<()> {
        let result =
            controller::transition_with(&self.story, &self.session, Intent::Advance, &self.logic)?;
        ensure!(result.entered, "视频 advance=true 需要可推进的剧情路由");
        self.enter(result.session, false, false)
    }
    pub(super) fn skip_video(&mut self, plan: Option<Plan>) -> Result<()> {
        let Some(cue) = self.video.cue.as_ref() else {
            anyhow::bail!("当前没有视频");
        };
        if !self.video.playing() || !cue.skippable {
            return Ok(());
        }
        if cue.advance {
            let result = controller::transition_with(
                &self.story,
                &self.session,
                Intent::Advance,
                &self.logic,
            )?;
            ensure!(result.entered, "视频跳过没有可推进路由");
            return self.enter_planned(result.session, false, false, plan);
        }
        if let Some(plan) = &plan {
            crate::effects::validate_portraits(
                &self.story.frames[&self.session.frame_id],
                &plan.portraits,
            )
            .map_err(anyhow::Error::msg)?;
            self.prepared_live2d
                .validate_commands(&self.story.frames[&self.session.frame_id], &plan.live2d)?;
        }
        self.video.skip();
        self.audio.video(None);
        if let Some(plan) = plan {
            self.commit_ui(plan, false);
        }
        Ok(())
    }
}
