//! Host-side keyboard ownership: raw frame snapshot, mapping and shortcut capture.
//!
//! The pure shortcut grammar lives in `crate::shortcuts`. This module only translates
//! egui/winit input into that vocabulary and owns transient capture state; it never
//! writes preferences or executes UI callbacks.
use crate::shortcuts::{
    Action, Binding, Key as SKey, Keybindings, Modifier, Modifiers as SModifiers, Trigger,
};
use eframe::egui;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: SKey,
    pub modifiers: SModifiers,
    pub pressed: bool,
    pub repeat: bool,
}

#[derive(Clone)]
pub struct FrameInput {
    pub events: Vec<KeyEvent>,
    pub held: BTreeSet<SKey>,
    pub modifiers: SModifiers,
    pub advance_pressed: bool,
    pub skip_sequence_pressed: bool,
    pub fast_forward_held: bool,
    pub fast_forward_held_raw: bool,
    pub fast_forward_released: bool,
    pub escape_pressed: bool,
    pub pointer_clicked: bool,
    pub any_key_down: bool,
    pub focused: bool,
}

fn modifiers(value: egui::Modifiers) -> SModifiers {
    SModifiers {
        ctrl: value.ctrl,
        alt: value.alt,
        shift: value.shift,
    }
}

/// Physical keys win. egui-winit cannot distinguish some numpad keys from their main-row
/// twins; those map to the main-row token and are documented as a host limitation.
pub fn map_key(key: egui::Key) -> Option<SKey> {
    use egui::Key as K;
    Some(match key {
        K::Space => SKey::Space,
        K::Enter => SKey::Enter,
        K::Tab => SKey::Tab,
        K::Backspace => SKey::Backspace,
        K::Delete => SKey::Delete,
        K::Insert => SKey::Insert,
        K::Home => SKey::Home,
        K::End => SKey::End,
        K::PageUp => SKey::PageUp,
        K::PageDown => SKey::PageDown,
        K::ArrowLeft => SKey::ArrowLeft,
        K::ArrowRight => SKey::ArrowRight,
        K::ArrowUp => SKey::ArrowUp,
        K::ArrowDown => SKey::ArrowDown,
        K::A => SKey::KeyA,
        K::B => SKey::KeyB,
        K::C => SKey::KeyC,
        K::D => SKey::KeyD,
        K::E => SKey::KeyE,
        K::F => SKey::KeyF,
        K::G => SKey::KeyG,
        K::H => SKey::KeyH,
        K::I => SKey::KeyI,
        K::J => SKey::KeyJ,
        K::K => SKey::KeyK,
        K::L => SKey::KeyL,
        K::M => SKey::KeyM,
        K::N => SKey::KeyN,
        K::O => SKey::KeyO,
        K::P => SKey::KeyP,
        K::Q => SKey::KeyQ,
        K::R => SKey::KeyR,
        K::S => SKey::KeyS,
        K::T => SKey::KeyT,
        K::U => SKey::KeyU,
        K::V => SKey::KeyV,
        K::W => SKey::KeyW,
        K::X => SKey::KeyX,
        K::Y => SKey::KeyY,
        K::Z => SKey::KeyZ,
        K::Num0 => SKey::Digit0,
        K::Num1 => SKey::Digit1,
        K::Num2 => SKey::Digit2,
        K::Num3 => SKey::Digit3,
        K::Num4 => SKey::Digit4,
        K::Num5 => SKey::Digit5,
        K::Num6 => SKey::Digit6,
        K::Num7 => SKey::Digit7,
        K::Num8 => SKey::Digit8,
        K::Num9 => SKey::Digit9,
        K::F1 => SKey::F1,
        K::F2 => SKey::F2,
        K::F3 => SKey::F3,
        K::F4 => SKey::F4,
        K::F5 => SKey::F5,
        K::F6 => SKey::F6,
        K::F7 => SKey::F7,
        K::F8 => SKey::F8,
        K::F9 => SKey::F9,
        K::F10 => SKey::F10,
        K::F11 => SKey::F11,
        K::F12 => SKey::F12,
        K::Minus => SKey::Minus,
        K::Equals => SKey::Equal,
        K::OpenBracket => SKey::BracketLeft,
        K::CloseBracket => SKey::BracketRight,
        K::Backslash => SKey::Backslash,
        K::Semicolon => SKey::Semicolon,
        K::Quote => SKey::Quote,
        K::Backtick => SKey::Backquote,
        K::Comma => SKey::Comma,
        K::Period => SKey::Period,
        K::Slash => SKey::Slash,
        // NumpadAdd is the only numpad key egui-winit maps distinctly to `Plus`.
        K::Plus => SKey::NumpadAdd,
        _ => return None,
    })
}

fn matches(binding: Binding, modifiers: SModifiers, key: SKey) -> bool {
    match binding.trigger {
        Trigger::Key {
            modifiers: wanted,
            key: wanted_key,
        } => wanted == modifiers && wanted_key == key,
        Trigger::Hold(_) => false,
    }
}

fn holds(binding: Binding, modifiers: SModifiers, held: &BTreeSet<SKey>) -> bool {
    match binding.trigger {
        Trigger::Hold(Modifier::Ctrl) => {
            modifiers
                == SModifiers {
                    ctrl: true,
                    alt: false,
                    shift: false,
                }
        }
        Trigger::Hold(Modifier::Alt) => {
            modifiers
                == SModifiers {
                    ctrl: false,
                    alt: true,
                    shift: false,
                }
        }
        Trigger::Hold(Modifier::Shift) => {
            modifiers
                == SModifiers {
                    ctrl: false,
                    alt: false,
                    shift: true,
                }
        }
        Trigger::Key {
            modifiers: wanted,
            key,
        } => modifiers == wanted && held.contains(&key),
    }
}

impl FrameInput {
    /// Reads one frame. `barrier` suppresses every mapped action until the user has released
    /// all physical keys; capture state is intentionally independent of this gate.
    pub fn collect(
        ctx: &egui::Context,
        bindings: &Keybindings,
        barrier: bool,
        held: &mut BTreeSet<SKey>,
    ) -> Self {
        let (events, keys_down, current, pointer_clicked, focused) = ctx.input(|input| {
            (
                input.events.clone(),
                input.keys_down.clone(),
                modifiers(input.modifiers),
                input.pointer.any_click(),
                input.focused,
            )
        });
        let mut mapped = Vec::new();
        let mut pressed = Vec::new();
        let mut escape_pressed = false;
        for event in events {
            if let egui::Event::Key {
                key,
                physical_key,
                pressed: is_pressed,
                repeat,
                modifiers: event_modifiers,
            } = event
            {
                if key == egui::Key::Escape && is_pressed && !repeat {
                    escape_pressed = true;
                }
                let Some(key) = map_key(physical_key.unwrap_or(key)) else {
                    continue;
                };
                let modifiers = modifiers(event_modifiers);
                if repeat {
                    continue;
                }
                if is_pressed {
                    held.insert(key);
                    pressed.push((key, modifiers));
                } else {
                    held.remove(&key);
                }
                mapped.push(KeyEvent {
                    key,
                    modifiers,
                    pressed: is_pressed,
                    repeat,
                });
            }
        }
        // Focus loss can swallow release events; never keep a stale physical hold.
        if !focused {
            held.clear();
        }
        let advance_pressed = !barrier
            && bindings.advance.is_some_and(|binding| {
                pressed
                    .iter()
                    .any(|(key, modifiers)| matches(binding, *modifiers, *key))
            });
        let skip_sequence_pressed = !barrier
            && bindings.skip_sequence.is_some_and(|binding| {
                pressed
                    .iter()
                    .any(|(key, modifiers)| matches(binding, *modifiers, *key))
            });
        let raw_fast_forward_held = bindings
            .fast_forward
            .is_some_and(|binding| holds(binding, current, held));
        let fast_forward_held = !barrier && raw_fast_forward_held;
        Self {
            events: mapped,
            held: held.clone(),
            modifiers: current,
            advance_pressed,
            skip_sequence_pressed,
            fast_forward_held,
            fast_forward_held_raw: raw_fast_forward_held,
            fast_forward_released: false,
            escape_pressed,
            pointer_clicked,
            any_key_down: !keys_down.is_empty() || current.any(),
            focused,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct ModBits {
    ctrl: bool,
    alt: bool,
    shift: bool,
}

impl ModBits {
    fn from(value: SModifiers) -> Self {
        Self {
            ctrl: value.ctrl,
            alt: value.alt,
            shift: value.shift,
        }
    }
    fn any(self) -> bool {
        self.ctrl || self.alt || self.shift
    }
    fn modifier(self) -> Option<Modifier> {
        match (self.ctrl, self.alt, self.shift) {
            (true, false, false) => Some(Modifier::Ctrl),
            (false, true, false) => Some(Modifier::Alt),
            (false, false, true) => Some(Modifier::Shift),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
enum Phase {
    /// Wait until the keys held when capture started are released, like the click itself.
    Arming,
    Armed,
    Waiting(Binding),
}

#[derive(Clone, Debug)]
pub struct Capture {
    pub action: Action,
    pub result_event: String,
    phase: Phase,
    last_modifiers: ModBits,
}

pub enum CaptureTick {
    Running,
    Complete(Binding),
    Cancelled,
}

impl Capture {
    pub fn new(action: Action, result_event: String) -> Self {
        Self {
            action,
            result_event,
            phase: Phase::Arming,
            last_modifiers: ModBits::default(),
        }
    }
    pub fn text(&self) -> &'static str {
        match self.phase {
            Phase::Arming => "请先松开当前按键…",
            Phase::Armed => "请按下新的快捷键；Escape 取消",
            Phase::Waiting(_) => "松开按键以确认；Escape 取消",
        }
    }
    pub fn tick(&mut self, input: &FrameInput) -> CaptureTick {
        if input.escape_pressed {
            self.phase = Phase::Arming;
            return CaptureTick::Cancelled;
        }
        let current = ModBits::from(input.modifiers);
        let released = ModBits {
            ctrl: self.last_modifiers.ctrl && !current.ctrl,
            alt: self.last_modifiers.alt && !current.alt,
            shift: self.last_modifiers.shift && !current.shift,
        };
        self.last_modifiers = current;
        match self.phase.clone() {
            Phase::Arming => {
                if !input.any_key_down && !current.any() {
                    self.phase = Phase::Armed;
                    return CaptureTick::Running;
                }
                CaptureTick::Running
            }
            Phase::Armed => {
                for event in &input.events {
                    if !event.pressed || event.repeat {
                        continue;
                    }
                    let candidate = Binding {
                        trigger: Trigger::Key {
                            modifiers: event.modifiers,
                            key: event.key,
                        },
                    };
                    if Binding::parse(&candidate.canonical()).is_err() {
                        continue;
                    }
                    self.phase = Phase::Waiting(candidate);
                    return CaptureTick::Running;
                }
                if !current.any()
                    && released.any()
                    && released_count(released) == 1
                    && self.action == Action::FastForward
                    && let Some(modifier) = released.modifier()
                {
                    let candidate = Binding {
                        trigger: Trigger::Hold(modifier),
                    };
                    self.phase = Phase::Waiting(candidate);
                    // The modifier is already released; deliver immediately.
                    return CaptureTick::Complete(candidate);
                }
                CaptureTick::Running
            }
            Phase::Waiting(candidate) => {
                // All combination keys must be lifted before one result is delivered.
                let done = match candidate.trigger {
                    Trigger::Key { modifiers, key } => {
                        !input.held.contains(&key)
                            && (!modifiers.ctrl || !input.modifiers.ctrl)
                            && (!modifiers.alt || !input.modifiers.alt)
                            && (!modifiers.shift || !input.modifiers.shift)
                    }
                    Trigger::Hold(modifier) => match modifier {
                        Modifier::Ctrl => !input.modifiers.ctrl,
                        Modifier::Alt => !input.modifiers.alt,
                        Modifier::Shift => !input.modifiers.shift,
                    },
                };
                if done && !input.any_key_down {
                    return CaptureTick::Complete(candidate);
                }
                CaptureTick::Running
            }
        }
    }
}

fn released_count(value: ModBits) -> u8 {
    u8::from(value.ctrl) + u8::from(value.alt) + u8::from(value.shift)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn physical_key_mapping_covers_the_configurable_table() {
        assert_eq!(map_key(egui::Key::Space), Some(SKey::Space));
        assert_eq!(map_key(egui::Key::A), Some(SKey::KeyA));
        assert_eq!(map_key(egui::Key::Num0), Some(SKey::Digit0));
        assert_eq!(map_key(egui::Key::Plus), Some(SKey::NumpadAdd));
        assert_eq!(map_key(egui::Key::Equals), Some(SKey::Equal));
        assert_eq!(map_key(egui::Key::Backtick), Some(SKey::Backquote));
        assert_eq!(map_key(egui::Key::Escape), None);
        assert_eq!(map_key(egui::Key::Copy), None);
    }

    #[test]
    fn capture_waits_for_release_then_confirms_a_chord() {
        let mut capture = Capture::new(Action::Advance, "result".into());
        let idle = FrameInput {
            events: Vec::new(),
            held: BTreeSet::new(),
            modifiers: SModifiers::default(),
            advance_pressed: false,
            skip_sequence_pressed: false,
            fast_forward_held: false,
            fast_forward_held_raw: false,
            fast_forward_released: false,
            escape_pressed: false,
            pointer_clicked: false,
            any_key_down: false,
            focused: true,
        };
        assert!(matches!(capture.tick(&idle), CaptureTick::Running));
        let press = FrameInput {
            events: vec![KeyEvent {
                key: SKey::Space,
                modifiers: SModifiers::default(),
                pressed: true,
                repeat: false,
            }],
            held: [SKey::Space].into_iter().collect(),
            any_key_down: true,
            ..idle.clone()
        };
        assert!(matches!(capture.tick(&press), CaptureTick::Running));
        let release = FrameInput {
            events: vec![KeyEvent {
                key: SKey::Space,
                modifiers: SModifiers::default(),
                pressed: false,
                repeat: false,
            }],
            held: BTreeSet::new(),
            ..idle.clone()
        };
        match capture.tick(&release) {
            CaptureTick::Complete(binding) => assert_eq!(binding.canonical(), "Space"),
            _ => panic!("capture should complete after release"),
        }
    }

    #[test]
    fn capture_ignores_plain_modifier_for_advance_but_accepts_it_for_fast_forward() {
        let idle = FrameInput {
            events: Vec::new(),
            held: BTreeSet::new(),
            modifiers: SModifiers::default(),
            advance_pressed: false,
            skip_sequence_pressed: false,
            fast_forward_held: false,
            fast_forward_held_raw: false,
            fast_forward_released: false,
            escape_pressed: false,
            pointer_clicked: false,
            any_key_down: false,
            focused: true,
        };
        let mut advance = Capture::new(Action::Advance, "result".into());
        advance.tick(&idle);
        let ctrl = FrameInput {
            modifiers: SModifiers {
                ctrl: true,
                ..Default::default()
            },
            any_key_down: true,
            ..idle.clone()
        };
        assert!(matches!(advance.tick(&ctrl), CaptureTick::Running));
        assert!(matches!(advance.tick(&idle), CaptureTick::Running));
        let mut fast = Capture::new(Action::FastForward, "result".into());
        fast.tick(&idle);
        fast.tick(&ctrl);
        match fast.tick(&idle) {
            CaptureTick::Complete(binding) => assert_eq!(binding.canonical(), "Ctrl"),
            _ => panic!("plain Ctrl should be captured for fast-forward"),
        }
    }
}
