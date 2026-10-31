//! Pure presentation math and transient clocks. No story execution or IO.
use crate::model::{Frame, Portrait, Slot, effects::*};
use std::collections::BTreeMap;
pub mod render;

pub fn ease(t: f32, e: Easing) -> f32 {
    let t = t.clamp(0.0, 1.0);
    match e {
        Easing::Linear => t,
        Easing::EaseIn => t * t,
        Easing::EaseOut => 1.0 - (1.0 - t).powi(2),
        Easing::EaseInOut => t * t * (3.0 - 2.0 * t),
    }
}
pub fn progress(age: f32, duration: u32) -> f32 {
    if duration == 0 {
        1.0
    } else {
        (age * 1000.0 / duration as f32).clamp(0.0, 1.0)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub rotation: f32,
    pub scale_x: f32,
    pub offset: [f32; 2],
    pub scale: f32,
    pub opacity: f32,
    pub flash: f32,
}
impl Default for Motion {
    fn default() -> Self {
        Self {
            rotation: 0.0,
            scale_x: 1.0,
            offset: [0.0; 2],
            scale: 1.0,
            opacity: 1.0,
            flash: 0.0,
        }
    }
}
impl Motion {
    pub fn combine(self, other: Self) -> Self {
        Self {
            offset: [
                self.offset[0] + other.offset[0],
                self.offset[1] + other.offset[1],
            ],
            scale: self.scale * other.scale,
            scale_x: self.scale_x * other.scale_x,
            rotation: self.rotation + other.rotation,
            opacity: self.opacity * other.opacity,
            flash: self.flash.max(other.flash),
        }
    }
}
pub fn pose_motion(pose: PortraitPose) -> Motion {
    let mut m = Motion::default();
    match pose {
        PortraitPose::Normal => {}
        PortraitPose::Flipped => m.scale_x = -1.0,
        PortraitPose::FallenLeft => {
            m.rotation = -90.0;
            m.offset[1] = 64.0;
        }
        PortraitPose::FallenRight => {
            m.rotation = 90.0;
            m.offset[1] = 64.0;
        }
        PortraitPose::Hidden => m.opacity = 0.0,
    }
    m
}
// Covers all three slots even with maximum authored scale/rotation and image animation.
const PORTRAIT_TRAVEL: f32 = 4096.0;
pub fn portrait_motion(pose: PortraitPose, a: PortraitAnimation, age: f32) -> Motion {
    if progress(age, a.duration) >= 1.0 {
        return pose_motion(pose);
    }
    if a.kind.is_exit() {
        return departing_motion(PortraitPose::Normal, a, age);
    }
    let mut m = pose_motion(pose);
    let raw = progress(age, a.duration);
    let t = ease(raw, a.easing);
    use PortraitActionKind::*;
    match a.kind {
        FadeIn => m.opacity *= t,
        EnterLeft => m.offset[0] -= PORTRAIT_TRAVEL * (1.0 - t),
        EnterRight => m.offset[0] += PORTRAIT_TRAVEL * (1.0 - t),
        EnterUp => m.offset[1] -= PORTRAIT_TRAVEL * (1.0 - t),
        EnterDown => m.offset[1] += PORTRAIT_TRAVEL * (1.0 - t),
        Shake => {
            m = m.combine(motion(
                Animation {
                    kind: AnimationKind::Shake,
                    duration: a.duration,
                    strength: a.strength,
                    easing: a.easing,
                },
                age,
            ))
        }
        Bounce => m.offset[1] -= a.strength * (std::f32::consts::PI * t).sin(),
        Turn => m.scale_x *= -(std::f32::consts::PI * t).cos(),
        Spin => m.scale_x *= (std::f32::consts::TAU * 2.0 * t).cos(),
        FallLeft | FallRight => {
            m.rotation *= t;
            m.offset[1] *= t;
        }
        _ => {}
    }
    m
}
pub fn departing_motion(source: PortraitPose, a: PortraitAnimation, age: f32) -> Motion {
    let mut m = pose_motion(source);
    let t = ease(progress(age, a.duration), a.easing);
    use PortraitActionKind::*;
    match a.kind {
        ExitLeft => m.offset[0] -= PORTRAIT_TRAVEL * t,
        ExitRight => m.offset[0] += PORTRAIT_TRAVEL * t,
        ExitUp => m.offset[1] -= PORTRAIT_TRAVEL * t,
        ExitDown => m.offset[1] += PORTRAIT_TRAVEL * t,
        _ => {}
    }
    m.opacity *= 1.0 - t;
    m
}
pub fn validate_portrait(a: PortraitAnimation) -> Result<(), String> {
    validate_animation(Animation {
        kind: AnimationKind::None,
        duration: a.duration,
        strength: a.strength,
        easing: a.easing,
    })
}
pub fn validate_portraits(
    frame: &Frame,
    actions: &[(Slot, PortraitAnimation)],
) -> Result<(), String> {
    for (slot, a) in actions {
        validate_portrait(*a)?;
        let p = frame
            .visual
            .portraits
            .iter()
            .find(|p| p.slot == *slot)
            .ok_or_else(|| {
                crate::i18n::format("host-effects-0001", &[("arg0", format!("{:?}", slot))])
            })?;
        if !a.kind.accepts(p.pose) {
            return Err(crate::i18n::text("host-effects-0002").into());
        }
    }
    Ok(())
}
pub fn motion(animation: Animation, age: f32) -> Motion {
    let raw = progress(age, animation.duration);
    let t = ease(raw, animation.easing);
    let mut m = Motion::default();
    if raw >= 1.0 {
        return m;
    }
    match animation.kind {
        AnimationKind::None => {}
        AnimationKind::FadeIn => m.opacity = t,
        AnimationKind::ZoomIn => m.scale = 0.85 + 0.15 * t,
        AnimationKind::ZoomOut => m.scale = 1.15 - 0.15 * t,
        AnimationKind::Shake => {
            m.offset = [
                (raw * std::f32::consts::TAU * 5.0).sin() * animation.strength * (1.0 - t),
                (raw * std::f32::consts::TAU * 7.0).sin() * animation.strength * 0.4 * (1.0 - t),
            ];
        }
        AnimationKind::Flash => m.flash = 1.0 - t,
        AnimationKind::SlideLeft => m.offset[0] = animation.strength * (1.0 - t),
        AnimationKind::SlideRight => m.offset[0] = -animation.strength * (1.0 - t),
        AnimationKind::SlideUp => m.offset[1] = animation.strength * (1.0 - t),
        AnimationKind::SlideDown => m.offset[1] = -animation.strength * (1.0 - t),
    }
    m
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layer {
    pub old: bool,
    pub offset: [f32; 2],
    pub clip: [f32; 4],
    pub opacity: f32,
}
pub struct Blend {
    pub layers: Vec<Layer>,
    pub veil: Option<([u8; 3], f32)>,
}
pub fn blend(kind: TransitionKind, t: f32) -> Blend {
    let t = t.clamp(0.0, 1.0);
    let base = Layer {
        old: false,
        offset: [0.0; 2],
        clip: [0.0, 0.0, 1.0, 1.0],
        opacity: 1.0,
    };
    let old = Layer { old: true, ..base };
    let mut result = Blend {
        layers: vec![base],
        veil: None,
    };
    if t >= 1.0 || kind == TransitionKind::None {
        return result;
    }
    result.layers = vec![old, base];
    match kind {
        TransitionKind::Dissolve => result.layers[1].opacity = t,
        TransitionKind::FadeBlack | TransitionKind::FadeWhite => {
            result.layers = vec![Layer {
                old: t < 0.5,
                ..base
            }];
            result.veil = Some((
                if kind == TransitionKind::FadeWhite {
                    [255; 3]
                } else {
                    [0; 3]
                },
                1.0 - (2.0 * t - 1.0).abs(),
            ));
        }
        TransitionKind::WipeLeft => result.layers[1].clip[0] = 1.0 - t,
        TransitionKind::WipeRight => result.layers[1].clip[2] = t,
        TransitionKind::WipeUp => result.layers[1].clip[1] = 1.0 - t,
        TransitionKind::WipeDown => result.layers[1].clip[3] = t,
        TransitionKind::SlideLeft
        | TransitionKind::SlideRight
        | TransitionKind::SlideUp
        | TransitionKind::SlideDown => {
            let axis = if matches!(kind, TransitionKind::SlideUp | TransitionKind::SlideDown) {
                1
            } else {
                0
            };
            let direction = if matches!(kind, TransitionKind::SlideLeft | TransitionKind::SlideUp) {
                -1.0
            } else {
                1.0
            };
            result.layers[0].offset[axis] = direction * t;
            result.layers[1].offset[axis] = -direction * (1.0 - t);
        }
        _ => {}
    }
    result
}
#[derive(Default)]
pub struct Player {
    pub ui_alpha: BTreeMap<String, f32>,
    pub portrait_actions: BTreeMap<Slot, (PortraitAnimation, f32)>,
    pub departing: Vec<Departure>,
    pub has_entered: bool,
    pub previous: Option<Frame>,
    pub transition: Transition,
    pub age: f32,
    pub ui_age: f32,
    pub animations: BTreeMap<String, (Animation, f32)>,
}
pub struct Departure {
    pub portrait: Portrait,
    pub animation: PortraitAnimation,
    pub age: f32,
}
impl Player {
    pub fn enter_scene(
        &mut self,
        previous: Option<Frame>,
        current: &Frame,
        transition: Transition,
        restored: bool,
    ) {
        self.enter(previous, transition, restored);
        if restored {
            return;
        }
        for p in &current.visual.portraits {
            if p.enter.kind != PortraitActionKind::None {
                self.portrait(p.slot, p.enter);
            }
        }
        if let Some(old) = &self.previous {
            for p in &old.visual.portraits {
                if p.pose != PortraitPose::Hidden
                    && p.exit.kind.is_exit()
                    && !current.visual.portraits.iter().any(|n| {
                        n.slot == p.slot && n.character == p.character && n.image == p.image
                    })
                {
                    let mut animation = p.exit;
                    if self.active() {
                        animation.duration = animation.duration.min(transition.duration);
                    }
                    self.departing.push(Departure {
                        portrait: p.clone(),
                        animation,
                        age: 0.0,
                    });
                }
            }
        }
    }
    pub fn portrait(&mut self, slot: Slot, animation: PortraitAnimation) {
        self.portrait_actions.insert(slot, (animation, 0.0));
    }
    pub fn portrait_motion(&self, p: &Portrait, old: bool) -> Motion {
        if old {
            return self
                .departing
                .iter()
                .find(|d| d.portrait.slot == p.slot)
                .map(|d| departing_motion(d.portrait.pose, d.animation, d.age))
                .unwrap_or_else(|| pose_motion(p.pose));
        }
        self.portrait_actions
            .get(&p.slot)
            .map(|(a, t)| portrait_motion(p.pose, *a, *t))
            .unwrap_or_else(|| pose_motion(p.pose))
    }
    pub fn active(&self) -> bool {
        self.transition.kind != TransitionKind::None
            && progress(self.age, self.transition.duration) < 1.0
    }
    pub fn enter(&mut self, previous: Option<Frame>, transition: Transition, restored: bool) {
        self.has_entered = true;
        self.previous = previous;
        self.transition = transition;
        self.age = if restored { 6.0 } else { 0.0 };
        self.ui_age = self.age;
        self.animations.clear();
        self.portrait_actions.clear();
        self.departing.clear();
    }
    pub fn finish(&mut self) {
        self.age = 6.0;
        self.departing.clear();
    }
    pub fn pages_changed(&mut self) {
        self.ui_age = 0.0;
        self.animations.clear();
    }
    pub fn tick(&mut self, seconds: f32) {
        let seconds = seconds.max(0.0);
        self.age += seconds;
        self.ui_age += seconds;
        for (_, age) in self.portrait_actions.values_mut() {
            *age += seconds;
        }
        self.portrait_actions
            .retain(|_, (a, t)| progress(*t, a.duration) < 1.0);
        for d in &mut self.departing {
            d.age += seconds;
        }
        let transitioning = self.active();
        self.departing
            .retain(|d| transitioning || progress(d.age, d.animation.duration) < 1.0);
        for (_, age) in self.animations.values_mut() {
            *age += seconds;
        }
        self.animations
            .retain(|_, (a, age)| progress(*age, a.duration) < 1.0);
        if !self.active() {
            self.previous = None;
        }
    }
    pub fn animate(&mut self, target: String, animation: Animation) {
        self.animations.insert(target, (animation, 0.0));
    }
    pub fn target_motion(&self, target: &str, fallback: Animation) -> Motion {
        self.animations
            .get(target)
            .map(|(a, t)| motion(*a, *t))
            .unwrap_or_else(|| motion(fallback, self.ui_age))
    }
    pub fn repaint(&self) -> bool {
        self.age < 5.0
            || self.ui_age < 5.0
            || !self.animations.is_empty()
            || !self.portrait_actions.is_empty()
            || !self.departing.is_empty()
    }
}

pub fn validate_transition(t: Transition) -> Result<(), String> {
    if t.duration > 5000 {
        Err(crate::i18n::text("host-effects-0003").into())
    } else {
        Ok(())
    }
}
pub fn validate_animation(a: Animation) -> Result<(), String> {
    if a.duration > 5000 || !a.strength.is_finite() || !(0.0..=128.0).contains(&a.strength) {
        Err(crate::i18n::text("host-effects-0004").into())
    } else {
        Ok(())
    }
}
