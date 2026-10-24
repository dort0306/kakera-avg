//! Serializable-looking authoring data only; animation clocks stay outside Model.
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PortraitPose {
    #[default]
    Normal,
    Flipped,
    FallenLeft,
    FallenRight,
    Hidden,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PortraitActionKind {
    #[default]
    None,
    FadeIn,
    FadeOut,
    EnterLeft,
    EnterRight,
    EnterUp,
    EnterDown,
    ExitLeft,
    ExitRight,
    ExitUp,
    ExitDown,
    Shake,
    Bounce,
    Turn,
    Spin,
    FallLeft,
    FallRight,
}
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortraitAnimation {
    pub kind: PortraitActionKind,
    pub duration: u32,
    pub strength: f32,
    pub easing: Easing,
}
impl Default for PortraitAnimation {
    fn default() -> Self {
        Self {
            kind: PortraitActionKind::None,
            duration: 600,
            strength: 32.0,
            easing: Easing::default(),
        }
    }
}
impl PortraitActionKind {
    pub fn is_exit(self) -> bool {
        matches!(
            self,
            Self::FadeOut | Self::ExitLeft | Self::ExitRight | Self::ExitUp | Self::ExitDown
        )
    }
    pub fn accepts(self, pose: PortraitPose) -> bool {
        match self {
            Self::None => true,
            Self::FallLeft => pose == PortraitPose::FallenLeft,
            Self::FallRight => pose == PortraitPose::FallenRight,
            Self::Turn => matches!(pose, PortraitPose::Normal | PortraitPose::Flipped),
            kind if kind.is_exit() => pose == PortraitPose::Hidden,
            _ => pose != PortraitPose::Hidden,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    #[default]
    EaseInOut,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TransitionKind {
    #[default]
    None,
    Dissolve,
    FadeBlack,
    FadeWhite,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub kind: TransitionKind,
    pub duration: u32,
    pub easing: Easing,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnimationKind {
    #[default]
    None,
    FadeIn,
    ZoomIn,
    ZoomOut,
    Shake,
    Flash,
    SlideLeft,
    SlideRight,
    SlideUp,
    SlideDown,
}
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Animation {
    pub kind: AnimationKind,
    pub duration: u32,
    pub strength: f32,
    pub easing: Easing,
}
impl Default for Animation {
    fn default() -> Self {
        Self {
            kind: AnimationKind::None,
            duration: 400,
            strength: 12.0,
            easing: Easing::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Filter {
    #[default]
    None,
    Grayscale,
    Sepia,
    Blur(u8),
    Pixelate(u8),
}
#[derive(Clone, Debug, PartialEq)]
pub struct ImageEffect {
    pub filter: Filter,
    pub opacity: f32,
    pub tint: [u8; 4],
    pub scale: f32,
    pub rotation: f32,
    pub flip_x: bool,
    pub flip_y: bool,
    pub animation: Animation,
}
impl Default for ImageEffect {
    fn default() -> Self {
        Self {
            filter: Filter::None,
            opacity: 1.0,
            tint: [255; 4],
            scale: 1.0,
            rotation: 0.0,
            flip_x: false,
            flip_y: false,
            animation: Animation::default(),
        }
    }
}
