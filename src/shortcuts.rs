//! 可配置快捷键的纯数据模块。
//!
//! 本模块只包含动作/按键枚举、绑定解析与校验，以及偏好 JSON 的编解码。
//! 不读写文件，也不引用 crate 内的其他模块；文件读取与原子替换由调用方负责。
//! 存储与比较使用固定的 ASCII token，界面显示文本不作为解析依据。

use std::fmt;

use serde::de::{Error as DeError, MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 单个绑定允许的最大 ASCII 字节数。
pub const MAX_BINDING_BYTES: usize = 64;

const KEYBINDINGS_SCHEMA_VERSION: u32 = 1;
const MAX_KEYBINDINGS_FILE_BYTES: usize = 16 * 1024;

/// 可由玩家重新绑定的剧情动作。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Advance,
    FastForward,
    SkipSequence,
}

impl Action {
    pub const ALL: [Self; 3] = [Self::Advance, Self::FastForward, Self::SkipSequence];

    /// 稳定动作 ID，用于配置文件与 JS 边界。
    pub fn id(self) -> &'static str {
        match self {
            Self::Advance => "advance",
            Self::FastForward => "fast_forward",
            Self::SkipSequence => "skip_sequence",
        }
    }

    /// 只接受稳定动作 ID，不做大小写或别名转换。
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "advance" => Some(Self::Advance),
            "fast_forward" => Some(Self::FastForward),
            "skip_sequence" => Some(Self::SkipSequence),
            _ => None,
        }
    }

    /// 用于提示与冲突信息的中文动作名。
    pub fn label(self) -> &'static str {
        match self {
            Self::Advance => "前进",
            Self::FastForward => "快进",
            Self::SkipSequence => "跳过演出",
        }
    }
}

/// 需要与主键同时按住的修饰键集合。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl Modifiers {
    pub fn any(self) -> bool {
        self.ctrl || self.alt || self.shift
    }

    /// 固定按 Ctrl、Alt、Shift 的顺序拼接，未按下任何修饰键时返回空串。
    pub fn canonical(self) -> String {
        let mut text = String::new();
        for (pressed, token) in [
            (self.ctrl, "Ctrl"),
            (self.alt, "Alt"),
            (self.shift, "Shift"),
        ] {
            if pressed {
                if !text.is_empty() {
                    text.push('+');
                }
                text.push_str(token);
            }
        }
        text
    }

    fn from_modifier(modifier: Modifier) -> Self {
        match modifier {
            Modifier::Ctrl => Self {
                ctrl: true,
                ..Self::default()
            },
            Modifier::Alt => Self {
                alt: true,
                ..Self::default()
            },
            Modifier::Shift => Self {
                shift: true,
                ..Self::default()
            },
        }
    }

    fn contains(self, modifier: Modifier) -> bool {
        match modifier {
            Modifier::Ctrl => self.ctrl,
            Modifier::Alt => self.alt,
            Modifier::Shift => self.shift,
        }
    }
}

/// 可以被单独按住的修饰键；第一版不区分左右键。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
}

impl Modifier {
    fn token(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
        }
    }

    fn modifiers(self) -> Modifiers {
        Modifiers::from_modifier(self)
    }
}

macro_rules! define_keys {
    ($( $variant:ident => ($token:literal, $display:expr), )+) => {
        /// 第一版允许绑定的全部主键；Escape 不在其中，留给快捷键捕获取消使用。
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Key {
            $( $variant, )+
        }

        impl Key {
            pub const ALL: &'static [Key] = &[$( Key::$variant, )+];

            /// 配置与比较使用的固定 ASCII token。
            pub fn token(self) -> &'static str {
                match self {
                    $( Key::$variant => $token, )+
                }
            }

            /// 只接受完全一致的 token，拒绝小写别名与首尾空白。
            pub fn from_token(token: &str) -> Option<Self> {
                match token {
                    $( $token => Some(Key::$variant), )+
                    _ => None,
                }
            }

            /// 供界面使用的易读名称；不接受它反向解析为键位。
            pub fn display(self) -> String {
                match self {
                    $( Key::$variant => $display.to_owned(), )+
                }
            }
        }
    };
}

define_keys! {
    Space => ("Space", crate::i18n::text("host-shortcuts-0020")),
    Enter => ("Enter", crate::i18n::text("host-shortcuts-0021")),
    Tab => ("Tab", crate::i18n::text("host-shortcuts-0022")),
    Backspace => ("Backspace", crate::i18n::text("host-shortcuts-0023")),
    Delete => ("Delete", crate::i18n::text("host-shortcuts-0024")),
    Insert => ("Insert", crate::i18n::text("host-shortcuts-0025")),
    Home => ("Home", crate::i18n::text("host-shortcuts-0026")),
    End => ("End", crate::i18n::text("host-shortcuts-0027")),
    PageUp => ("PageUp", crate::i18n::text("host-shortcuts-0028")),
    PageDown => ("PageDown", crate::i18n::text("host-shortcuts-0029")),
    ArrowLeft => ("ArrowLeft", crate::i18n::text("host-shortcuts-0030")),
    ArrowRight => ("ArrowRight", crate::i18n::text("host-shortcuts-0031")),
    ArrowUp => ("ArrowUp", crate::i18n::text("host-shortcuts-0032")),
    ArrowDown => ("ArrowDown", crate::i18n::text("host-shortcuts-0033")),
    KeyA => ("KeyA", "A"),
    KeyB => ("KeyB", "B"),
    KeyC => ("KeyC", "C"),
    KeyD => ("KeyD", "D"),
    KeyE => ("KeyE", "E"),
    KeyF => ("KeyF", "F"),
    KeyG => ("KeyG", "G"),
    KeyH => ("KeyH", "H"),
    KeyI => ("KeyI", "I"),
    KeyJ => ("KeyJ", "J"),
    KeyK => ("KeyK", "K"),
    KeyL => ("KeyL", "L"),
    KeyM => ("KeyM", "M"),
    KeyN => ("KeyN", "N"),
    KeyO => ("KeyO", "O"),
    KeyP => ("KeyP", "P"),
    KeyQ => ("KeyQ", "Q"),
    KeyR => ("KeyR", "R"),
    KeyS => ("KeyS", "S"),
    KeyT => ("KeyT", "T"),
    KeyU => ("KeyU", "U"),
    KeyV => ("KeyV", "V"),
    KeyW => ("KeyW", "W"),
    KeyX => ("KeyX", "X"),
    KeyY => ("KeyY", "Y"),
    KeyZ => ("KeyZ", "Z"),
    Digit0 => ("Digit0", "0"),
    Digit1 => ("Digit1", "1"),
    Digit2 => ("Digit2", "2"),
    Digit3 => ("Digit3", "3"),
    Digit4 => ("Digit4", "4"),
    Digit5 => ("Digit5", "5"),
    Digit6 => ("Digit6", "6"),
    Digit7 => ("Digit7", "7"),
    Digit8 => ("Digit8", "8"),
    Digit9 => ("Digit9", "9"),
    F1 => ("F1", crate::i18n::text("host-shortcuts-0034")),
    F2 => ("F2", crate::i18n::text("host-shortcuts-0035")),
    F3 => ("F3", crate::i18n::text("host-shortcuts-0036")),
    F4 => ("F4", crate::i18n::text("host-shortcuts-0037")),
    F5 => ("F5", crate::i18n::text("host-shortcuts-0038")),
    F6 => ("F6", crate::i18n::text("host-shortcuts-0039")),
    F7 => ("F7", crate::i18n::text("host-shortcuts-0040")),
    F8 => ("F8", crate::i18n::text("host-shortcuts-0041")),
    F9 => ("F9", crate::i18n::text("host-shortcuts-0042")),
    F10 => ("F10", crate::i18n::text("host-shortcuts-0043")),
    F11 => ("F11", crate::i18n::text("host-shortcuts-0044")),
    F12 => ("F12", crate::i18n::text("host-shortcuts-0045")),
    Minus => ("Minus", crate::i18n::text("host-shortcuts-0046")),
    Equal => ("Equal", crate::i18n::text("host-shortcuts-0047")),
    BracketLeft => ("BracketLeft", crate::i18n::text("host-shortcuts-0048")),
    BracketRight => ("BracketRight", crate::i18n::text("host-shortcuts-0049")),
    Backslash => ("Backslash", crate::i18n::text("host-shortcuts-0050")),
    Semicolon => ("Semicolon", crate::i18n::text("host-shortcuts-0051")),
    Quote => ("Quote", crate::i18n::text("host-shortcuts-0052")),
    Backquote => ("Backquote", crate::i18n::text("host-shortcuts-0053")),
    Comma => ("Comma", crate::i18n::text("host-shortcuts-0054")),
    Period => ("Period", crate::i18n::text("host-shortcuts-0055")),
    Slash => ("Slash", crate::i18n::text("host-shortcuts-0056")),
    Numpad0 => ("Numpad0", crate::i18n::text("host-shortcuts-0057")),
    Numpad1 => ("Numpad1", crate::i18n::text("host-shortcuts-0058")),
    Numpad2 => ("Numpad2", crate::i18n::text("host-shortcuts-0059")),
    Numpad3 => ("Numpad3", crate::i18n::text("host-shortcuts-0060")),
    Numpad4 => ("Numpad4", crate::i18n::text("host-shortcuts-0061")),
    Numpad5 => ("Numpad5", crate::i18n::text("host-shortcuts-0062")),
    Numpad6 => ("Numpad6", crate::i18n::text("host-shortcuts-0063")),
    Numpad7 => ("Numpad7", crate::i18n::text("host-shortcuts-0064")),
    Numpad8 => ("Numpad8", crate::i18n::text("host-shortcuts-0065")),
    Numpad9 => ("Numpad9", crate::i18n::text("host-shortcuts-0066")),
    NumpadAdd => ("NumpadAdd", crate::i18n::text("host-shortcuts-0067")),
    NumpadSubtract => ("NumpadSubtract", crate::i18n::text("host-shortcuts-0068")),
    NumpadMultiply => ("NumpadMultiply", crate::i18n::text("host-shortcuts-0069")),
    NumpadDivide => ("NumpadDivide", crate::i18n::text("host-shortcuts-0070")),
    NumpadDecimal => ("NumpadDecimal", crate::i18n::text("host-shortcuts-0071")),
    NumpadEnter => ("NumpadEnter", crate::i18n::text("host-shortcuts-0072")),
}

/// 一个绑定的触发方式：组合键的按下边沿，或按住单个修饰键。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Trigger {
    Key { modifiers: Modifiers, key: Key },
    Hold(Modifier),
}

/// 规范化后的单个键位绑定。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Binding {
    pub trigger: Trigger,
}

impl Binding {
    /// 解析用户输入的规范/非规范顺序组合。
    ///
    /// 只接受 ASCII 固定 token；拒绝空白、空段、重复 token、多段序列、
    /// Escape/Windows/Meta、Ctrl+Alt 及系统保留组合。
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.is_empty() {
            return Err(crate::i18n::text("host-shortcuts-0073").to_owned());
        }
        if text.len() > MAX_BINDING_BYTES {
            return Err(crate::i18n::format(
                "host-shortcuts-0001",
                &[
                    ("arg0", MAX_BINDING_BYTES.to_string()),
                    ("arg1", format!("{}", text.len())),
                ],
            ));
        }
        if !text.is_ascii() {
            return Err(crate::i18n::text("host-shortcuts-0074").to_owned());
        }
        if text.chars().any(char::is_whitespace) {
            return Err(crate::i18n::text("host-shortcuts-0075").to_owned());
        }

        let parts: Vec<&str> = text.split('+').collect();
        if parts.len() > 4 {
            return Err(crate::i18n::text("host-shortcuts-0076").to_owned());
        }
        if parts.iter().any(|part| part.is_empty()) {
            return Err(crate::i18n::text("host-shortcuts-0077").to_owned());
        }
        for (index, part) in parts.iter().enumerate() {
            if parts[..index].contains(part) {
                return Err(crate::i18n::format(
                    "host-shortcuts-0002",
                    &[("arg0", part.to_string())],
                ));
            }
            if let Some(reason) = reserved_token_reason(part) {
                return Err(reason.to_owned());
            }
        }

        let has_ctrl = parts.contains(&"Ctrl");
        let has_alt = parts.contains(&"Alt");
        if has_ctrl && has_alt {
            return Err(crate::i18n::text("host-shortcuts-0078").to_owned());
        }

        if parts.len() == 1 {
            let token = parts[0];
            if let Some(modifier) = parse_modifier(token) {
                return Ok(Self {
                    trigger: Trigger::Hold(modifier),
                });
            }
            let Some(key) = Key::from_token(token) else {
                return Err(crate::i18n::format(
                    "host-shortcuts-0003",
                    &[("arg0", token.to_string())],
                ));
            };
            return Ok(Self {
                trigger: Trigger::Key {
                    modifiers: Modifiers::default(),
                    key,
                },
            });
        }

        let mut modifiers = Modifiers::default();
        for token in &parts[..parts.len() - 1] {
            let Some(modifier) = parse_modifier(token) else {
                return Err(crate::i18n::format(
                    "host-shortcuts-0004",
                    &[("arg0", token.to_string())],
                ));
            };
            match modifier {
                Modifier::Ctrl => modifiers.ctrl = true,
                Modifier::Alt => modifiers.alt = true,
                Modifier::Shift => modifiers.shift = true,
            }
        }

        let key_token = parts[parts.len() - 1];
        let Some(key) = Key::from_token(key_token) else {
            if parse_modifier(key_token).is_some() {
                return Err(crate::i18n::text("host-shortcuts-0079").to_owned());
            }
            return Err(crate::i18n::format(
                "host-shortcuts-0005",
                &[("arg0", key_token.to_string())],
            ));
        };
        if modifiers.alt && matches!(key, Key::F4 | Key::Tab | Key::Space) {
            return Err(crate::i18n::text("host-shortcuts-0080").to_owned());
        }

        Ok(Self {
            trigger: Trigger::Key { modifiers, key },
        })
    }

    /// 配置与比较使用的规范字符串。
    pub fn canonical(&self) -> String {
        match self.trigger {
            Trigger::Key { modifiers, key } => {
                let mut text = modifiers.canonical();
                if !text.is_empty() {
                    text.push('+');
                }
                text.push_str(key.token());
                text
            }
            Trigger::Hold(modifier) => modifier.token().to_owned(),
        }
    }

    /// 面向玩家的易读文本，不能作为存储格式回解析。
    pub fn display(&self) -> String {
        match self.trigger {
            Trigger::Key { modifiers, key } => {
                let prefix = modifiers.canonical();
                if prefix.is_empty() {
                    key.display()
                } else {
                    format!("{prefix}+{}", key.display())
                }
            }
            Trigger::Hold(modifier) => crate::i18n::format(
                "host-shortcuts-0006",
                &[("arg0", modifier.token().to_string())],
            ),
        }
    }

    pub fn is_hold(&self) -> bool {
        matches!(self.trigger, Trigger::Hold(_))
    }

    pub fn key(&self) -> Option<Key> {
        match self.trigger {
            Trigger::Key { key, .. } => Some(key),
            Trigger::Hold(_) => None,
        }
    }

    pub fn modifiers(&self) -> Modifiers {
        match self.trigger {
            Trigger::Key { modifiers, .. } => modifiers,
            Trigger::Hold(modifier) => modifier.modifiers(),
        }
    }
}

fn parse_modifier(token: &str) -> Option<Modifier> {
    match token {
        "Ctrl" => Some(Modifier::Ctrl),
        "Alt" => Some(Modifier::Alt),
        "Shift" => Some(Modifier::Shift),
        _ => None,
    }
}

fn reserved_token_reason(token: &str) -> Option<&'static str> {
    match token {
        "Escape" => Some("Escape 保留给捕获取消，不能绑定"),
        "Meta" | "Windows" => Some("Windows/Meta 键及其组合属于系统保留组合，不能绑定"),
        _ => None,
    }
}

/// 三个剧情动作的绑定表。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keybindings {
    pub advance: Option<Binding>,
    pub fast_forward: Option<Binding>,
    pub skip_sequence: Option<Binding>,
}

impl Keybindings {
    /// 方案第 2 节的默认映射。
    pub fn defaults() -> Self {
        Self {
            advance: Some(Binding {
                trigger: Trigger::Key {
                    modifiers: Modifiers::default(),
                    key: Key::Space,
                },
            }),
            fast_forward: Some(Binding {
                trigger: Trigger::Hold(Modifier::Ctrl),
            }),
            skip_sequence: Some(Binding {
                trigger: Trigger::Key {
                    modifiers: Modifiers::default(),
                    key: Key::Space,
                },
            }),
        }
    }

    pub fn get(&self, action: Action) -> Option<Binding> {
        match action {
            Action::Advance => self.advance,
            Action::FastForward => self.fast_forward,
            Action::SkipSequence => self.skip_sequence,
        }
    }

    pub fn set(&mut self, action: Action, value: Option<Binding>) {
        match action {
            Action::Advance => self.advance = value,
            Action::FastForward => self.fast_forward = value,
            Action::SkipSequence => self.skip_sequence = value,
        }
    }

    /// 校验完整候选映射：只允许快进按住修饰键，并按上下文检查前缀冲突。
    pub fn validate(&self) -> Result<(), String> {
        validate_binding(Action::Advance, self.advance)?;
        validate_binding(Action::FastForward, self.fast_forward)?;
        validate_binding(Action::SkipSequence, self.skip_sequence)?;
        self.validate_conflicts()
    }

    fn validate_conflicts(&self) -> Result<(), String> {
        let (Some(advance), Some(fast_forward)) = (self.advance, self.fast_forward) else {
            return Ok(());
        };
        if advance == fast_forward {
            return Err(crate::i18n::format(
                "host-shortcuts-0007",
                &[
                    ("arg0", Action::Advance.label().to_string()),
                    ("arg1", Action::FastForward.label().to_string()),
                    ("arg2", advance.display().to_string()),
                ],
            ));
        }
        if let Trigger::Hold(modifier) = fast_forward.trigger
            && advance.modifiers().contains(modifier)
        {
            return Err(crate::i18n::format(
                "host-shortcuts-0008",
                &[
                    ("arg0", Action::FastForward.label().to_string()),
                    ("arg1", Action::Advance.label().to_string()),
                    ("arg2", modifier.token().to_string()),
                    ("arg3", Action::Advance.label().to_string()),
                    ("arg4", advance.display().to_string()),
                ],
            ));
        }
        Ok(())
    }

    /// 未绑定时返回“未绑定”，否则返回该绑定的易读文本。
    pub fn display(&self, action: Action) -> String {
        match self.get(action) {
            Some(binding) => binding.display(),
            None => crate::i18n::text("host-shortcuts-0081").to_owned(),
        }
    }

    /// 只包含三个动作字段的对象；值恒为 null 或规范字符串。
    pub fn to_json(&self) -> Result<String, String> {
        self.validate()?;
        serde_json::to_string(self).map_err(|error| {
            crate::i18n::format("host-shortcuts-0009", &[("arg0", error.to_string())])
        })
    }

    /// 严格解析三动作对象，缺少/多余/重复字段或非法绑定均拒绝。
    pub fn from_json(text: &str) -> Result<Self, String> {
        let bindings: Self = serde_json::from_str(text)
            .map_err(|error| json_parse_error(crate::i18n::text("host-shortcuts-0082"), &error))?;
        bindings.validate()?;
        Ok(bindings)
    }
}

fn validate_binding(action: Action, binding: Option<Binding>) -> Result<(), String> {
    let Some(binding) = binding else {
        return Ok(());
    };
    let canonical = binding.canonical();
    Binding::parse(&canonical).map_err(|reason| {
        crate::i18n::format(
            "host-shortcuts-0010",
            &[
                ("arg0", action.label().to_string()),
                ("arg1", reason.to_string()),
            ],
        )
    })?;
    if action != Action::FastForward
        && let Trigger::Hold(modifier) = binding.trigger
    {
        return Err(crate::i18n::format(
            "host-shortcuts-0011",
            &[
                ("arg0", action.label().to_string()),
                ("arg1", modifier.token().to_string()),
                ("arg2", Action::FastForward.label().to_string()),
            ],
        ));
    }
    Ok(())
}

impl Serialize for Keybindings {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("Keybindings", 3)?;
        state.serialize_field("advance", &self.advance.map(|binding| binding.canonical()))?;
        state.serialize_field(
            "fast_forward",
            &self.fast_forward.map(|binding| binding.canonical()),
        )?;
        state.serialize_field(
            "skip_sequence",
            &self.skip_sequence.map(|binding| binding.canonical()),
        )?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Keybindings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(
            "Keybindings",
            &["advance", "fast_forward", "skip_sequence"],
            KeybindingsVisitor,
        )
    }
}

struct KeybindingsVisitor;

impl<'de> Visitor<'de> for KeybindingsVisitor {
    type Value = Keybindings;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(crate::i18n::text("host-shortcuts-0083"))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E>
    where
        E: DeError,
    {
        Err(E::custom(crate::i18n::text("host-shortcuts-0084")))
    }

    fn visit_seq<A>(self, _sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        Err(A::Error::custom(crate::i18n::text("host-shortcuts-0085")))
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut advance: Option<OptionalBindingText> = None;
        let mut fast_forward: Option<OptionalBindingText> = None;
        let mut skip_sequence: Option<OptionalBindingText> = None;

        while let Some(field) = map.next_key::<String>()? {
            match field.as_str() {
                "advance" => {
                    if advance.is_some() {
                        return Err(A::Error::custom(crate::i18n::text("host-shortcuts-0086")));
                    }
                    advance = Some(map.next_value()?);
                }
                "fast_forward" => {
                    if fast_forward.is_some() {
                        return Err(A::Error::custom(crate::i18n::text("host-shortcuts-0087")));
                    }
                    fast_forward = Some(map.next_value()?);
                }
                "skip_sequence" => {
                    if skip_sequence.is_some() {
                        return Err(A::Error::custom(crate::i18n::text("host-shortcuts-0088")));
                    }
                    skip_sequence = Some(map.next_value()?);
                }
                other => {
                    return Err(A::Error::custom(crate::i18n::format(
                        "host-shortcuts-0012",
                        &[("arg0", other.to_string())],
                    )));
                }
            }
        }

        let advance =
            advance.ok_or_else(|| A::Error::custom(crate::i18n::text("host-shortcuts-0089")))?;
        let fast_forward = fast_forward
            .ok_or_else(|| A::Error::custom(crate::i18n::text("host-shortcuts-0090")))?;
        let skip_sequence = skip_sequence
            .ok_or_else(|| A::Error::custom(crate::i18n::text("host-shortcuts-0091")))?;

        let bindings = Keybindings {
            advance: parse_binding_field(advance, "advance").map_err(A::Error::custom)?,
            fast_forward: parse_binding_field(fast_forward, "fast_forward")
                .map_err(A::Error::custom)?,
            skip_sequence: parse_binding_field(skip_sequence, "skip_sequence")
                .map_err(A::Error::custom)?,
        };
        bindings.validate().map_err(A::Error::custom)?;
        Ok(bindings)
    }
}

/// 区分“字段缺失/值为 null”和“字段存在但是字符串”。
struct OptionalBindingText(Option<String>);

impl<'de> Deserialize<'de> for OptionalBindingText {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct OptionalBindingTextVisitor;

        impl<'de> Visitor<'de> for OptionalBindingTextVisitor {
            type Value = OptionalBindingText;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(crate::i18n::text("host-shortcuts-0092"))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                Ok(OptionalBindingText(None))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                Ok(OptionalBindingText(None))
            }

            fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_any(OptionalBindingTextVisitor)
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                Ok(OptionalBindingText(Some(value.to_owned())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                Ok(OptionalBindingText(Some(value)))
            }

            fn visit_bool<E>(self, _value: bool) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                Err(E::custom(crate::i18n::text("host-shortcuts-0093")))
            }

            fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                Err(E::custom(crate::i18n::text("host-shortcuts-0094")))
            }

            fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                Err(E::custom(crate::i18n::text("host-shortcuts-0095")))
            }

            fn visit_f64<E>(self, _value: f64) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                Err(E::custom(crate::i18n::text("host-shortcuts-0096")))
            }

            fn visit_seq<A>(self, _sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                Err(A::Error::custom(crate::i18n::text("host-shortcuts-0097")))
            }

            fn visit_map<A>(self, _map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                Err(A::Error::custom(crate::i18n::text("host-shortcuts-0098")))
            }
        }

        deserializer.deserialize_any(OptionalBindingTextVisitor)
    }
}

fn parse_binding_field(value: OptionalBindingText, field: &str) -> Result<Option<Binding>, String> {
    match value.0 {
        None => Ok(None),
        Some(text) => Binding::parse(&text).map(Some).map_err(|reason| {
            crate::i18n::format(
                "host-shortcuts-0013",
                &[
                    ("arg0", field.to_string()),
                    ("arg1", text.to_string()),
                    ("arg2", reason.to_string()),
                ],
            )
        }),
    }
}

/// 独立快捷键偏好文件的完整结构。
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeybindingsFile {
    pub schema_version: u32,
    pub story_id: String,
    pub bindings: Keybindings,
}

impl KeybindingsFile {
    /// 严格读取偏好文件字节；大小、版本、story_id 与完整键位图都验证后才返回绑定。
    pub fn load_bytes(bytes: &[u8], story_id: &str) -> Result<Keybindings, String> {
        if bytes.len() > MAX_KEYBINDINGS_FILE_BYTES {
            return Err(crate::i18n::format(
                "host-shortcuts-0014",
                &[("arg0", format!("{}", bytes.len()))],
            ));
        }
        let file: Self = serde_json::from_slice(bytes)
            .map_err(|error| json_parse_error(crate::i18n::text("host-shortcuts-0099"), &error))?;
        if file.schema_version != KEYBINDINGS_SCHEMA_VERSION {
            return Err(crate::i18n::format(
                "host-shortcuts-0015",
                &[
                    ("arg0", file.schema_version.to_string()),
                    ("arg1", KEYBINDINGS_SCHEMA_VERSION.to_string()),
                ],
            ));
        }
        if file.story_id != story_id {
            return Err(crate::i18n::format(
                "host-shortcuts-0016",
                &[
                    ("arg0", story_id.to_string()),
                    ("arg1", file.story_id.to_string()),
                ],
            ));
        }
        file.bindings.validate()?;
        Ok(file.bindings)
    }

    /// 生成 schema_version=1 的完整偏好字节；序列化结果同样受 16KiB 上限约束。
    pub fn to_bytes(story_id: &str, bindings: &Keybindings) -> Result<Vec<u8>, String> {
        bindings.validate()?;
        let file = Self {
            schema_version: KEYBINDINGS_SCHEMA_VERSION,
            story_id: story_id.to_owned(),
            bindings: *bindings,
        };
        let bytes = serde_json::to_vec(&file).map_err(|error| {
            crate::i18n::format("host-shortcuts-0017", &[("arg0", error.to_string())])
        })?;
        if bytes.len() > MAX_KEYBINDINGS_FILE_BYTES {
            return Err(crate::i18n::format(
                "host-shortcuts-0018",
                &[("arg0", format!("{}", bytes.len()))],
            ));
        }
        Ok(bytes)
    }
}

fn json_parse_error(prefix: &str, error: &serde_json::Error) -> String {
    let category = if error.is_io() {
        crate::i18n::text("host-shortcuts-0100")
    } else if error.is_syntax() {
        crate::i18n::text("host-shortcuts-0101")
    } else if error.is_data() {
        crate::i18n::text("host-shortcuts-0102")
    } else {
        crate::i18n::text("host-shortcuts-0103")
    };
    crate::i18n::format(
        "host-shortcuts-0019",
        &[
            ("arg0", prefix.to_string()),
            ("arg1", category.to_string()),
            ("arg2", error.to_string()),
        ],
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    fn binding(text: &str) -> Binding {
        Binding::parse(text).expect("测试绑定应当合法")
    }

    /// 方案中的序列化示例：前进 Space、快进按住 Ctrl、跳过演出未绑定。
    fn example_bindings() -> Keybindings {
        Keybindings {
            advance: Some(binding("Space")),
            fast_forward: Some(binding("Ctrl")),
            skip_sequence: None,
        }
    }

    fn reject(text: &str) -> String {
        match Binding::parse(text) {
            Ok(binding) => panic!("应拒绝「{text}」，却解析为「{}」", binding.canonical()),
            Err(reason) => {
                assert!(!reason.is_empty(), "拒绝原因不能为空");
                reason
            }
        }
    }

    #[test]
    fn defaults_and_action_table() {
        let bindings = Keybindings::defaults();
        assert_eq!(bindings.get(Action::Advance), Some(binding("Space")));
        assert_eq!(bindings.get(Action::FastForward), Some(binding("Ctrl")));
        assert_eq!(bindings.get(Action::SkipSequence), Some(binding("Space")));
        assert_eq!(
            bindings.fast_forward.map(|value| value.trigger),
            Some(Trigger::Hold(Modifier::Ctrl))
        );
        assert!(bindings.validate().is_ok());

        assert_eq!(Action::ALL.len(), 3);
        for action in Action::ALL {
            assert_eq!(Action::from_id(action.id()), Some(action));
            assert!(!action.label().is_empty());
            assert!(!action.label().is_ascii());
        }
        assert!(Action::from_id("Advance").is_none());
        assert!(Action::from_id("skip").is_none());

        let mut changed = bindings;
        changed.set(Action::Advance, None);
        assert_eq!(changed.get(Action::Advance), None);
        changed.set(Action::Advance, Some(binding("Enter")));
        assert_eq!(changed.get(Action::Advance), Some(binding("Enter")));
    }

    #[test]
    fn key_table_is_complete_and_round_trips() {
        assert_eq!(Key::ALL.len(), 89);
        let mut tokens = HashSet::new();
        for &key in Key::ALL {
            assert!(tokens.insert(key.token()), "token 重复：{}", key.token());
            assert_eq!(Key::from_token(key.token()), Some(key));
            assert!(!key.display().is_empty());
            assert!(
                Binding::parse(&key.display()).is_err(),
                "显示名称不能作为存储 token：{}",
                key.display()
            );

            let value = Binding {
                trigger: Trigger::Key {
                    modifiers: Modifiers::default(),
                    key,
                },
            };
            assert_eq!(value.canonical(), key.token());
            assert_eq!(Binding::parse(&value.canonical()), Ok(value));

            let bindings = Keybindings {
                advance: Some(value),
                fast_forward: None,
                skip_sequence: None,
            };
            let json = bindings.to_json().expect("单个键位应可序列化");
            assert_eq!(
                Keybindings::from_json(&json).expect("单个键位应可反序列化"),
                bindings
            );
        }
        assert_eq!(tokens.len(), Key::ALL.len());
        assert!(Key::from_token("Escape").is_none());
        assert!(Key::from_token("space").is_none());
        assert!(Key::from_token("KeyAA").is_none());
        assert!(Key::from_token("NumpadEqual").is_none());
        for token in [
            "Space",
            "Enter",
            "Tab",
            "Backspace",
            "Delete",
            "Insert",
            "Home",
            "End",
            "PageUp",
            "PageDown",
            "ArrowLeft",
            "ArrowRight",
            "ArrowUp",
            "ArrowDown",
            "KeyA",
            "KeyZ",
            "Digit0",
            "Digit9",
            "F1",
            "F12",
            "Minus",
            "Equal",
            "BracketLeft",
            "BracketRight",
            "Backslash",
            "Semicolon",
            "Quote",
            "Backquote",
            "Comma",
            "Period",
            "Slash",
            "Numpad0",
            "Numpad9",
            "NumpadAdd",
            "NumpadSubtract",
            "NumpadMultiply",
            "NumpadDivide",
            "NumpadDecimal",
            "NumpadEnter",
        ] {
            assert!(Key::from_token(token).is_some(), "缺少主键 token：{token}");
        }
    }

    #[test]
    fn modifiers_and_orders_are_canonical() {
        assert_eq!(Modifiers::default().canonical(), "");
        assert_eq!(
            Modifiers {
                ctrl: true,
                alt: false,
                shift: false,
            }
            .canonical(),
            "Ctrl"
        );
        assert_eq!(
            Modifiers {
                ctrl: false,
                alt: true,
                shift: true,
            }
            .canonical(),
            "Alt+Shift"
        );
        assert_eq!(
            Modifiers {
                ctrl: true,
                alt: true,
                shift: true,
            }
            .canonical(),
            "Ctrl+Alt+Shift"
        );
        assert!(!Modifiers::default().any());
        assert!(
            Modifiers {
                ctrl: false,
                alt: false,
                shift: true,
            }
            .any()
        );

        assert_eq!(binding("KeyK").canonical(), "KeyK");
        assert_eq!(binding("Ctrl+KeyK").canonical(), "Ctrl+KeyK");
        assert_eq!(binding("Shift+Ctrl+KeyK").canonical(), "Ctrl+Shift+KeyK");
        assert_eq!(binding("Shift+Alt+KeyK").canonical(), "Alt+Shift+KeyK");
        assert_eq!(binding("Shift+Ctrl+KeyK"), binding("Ctrl+Shift+KeyK"));
        assert_eq!(
            binding("Shift+Ctrl+KeyK").modifiers(),
            Modifiers {
                ctrl: true,
                alt: false,
                shift: true,
            }
        );
        assert_eq!(binding("Ctrl").canonical(), "Ctrl");
        assert!(binding("Ctrl").is_hold());
        assert_eq!(binding("Ctrl").key(), None);
        assert_eq!(binding("Space").key(), Some(Key::Space));
        assert!(!binding("Space").is_hold());
    }

    #[test]
    fn invalid_bindings_are_rejected() {
        for text in [
            "",
            "+",
            "++",
            "Ctrl+",
            "+Ctrl",
            "Ctrl++KeyK",
            "Ctrl +KeyK",
            "Ctrl+ KeyK",
            "Ctrl+KeyK ",
            " Space",
            "space",
            "SPACE",
            "ctrl+KeyK",
            "ctrl",
            "shift+KeyK",
            "Ctrl+Ctrl+KeyK",
            "KeyA+KeyA",
            "Shift+Shift+KeyA",
            "Ctrl+KeyK+KeyJ",
            "Space+Ctrl+KeyK",
            "Ctrl+Shift",
            "Ctrl+Unknown",
            "Unknown+KeyK",
            "Ctrl+KeyAA",
            "Ctrl+NumpadEqual",
            "Ctrl+键A",
        ] {
            reject(text);
        }

        let overlong = "A".repeat(MAX_BINDING_BYTES + 1);
        let reason = reject(&overlong);
        assert!(reason.contains("64"));
        let at_limit = "A".repeat(MAX_BINDING_BYTES);
        reject(&at_limit);
    }

    #[test]
    fn reserved_bindings_are_rejected() {
        for text in [
            "Escape",
            "Meta",
            "Windows",
            "Meta+KeyA",
            "Windows+KeyA",
            "Ctrl+Escape",
            "Ctrl+Shift+Escape",
            "Alt+Escape",
            "Alt+F4",
            "Alt+Tab",
            "Alt+Space",
            "Shift+Alt+Tab",
            "Ctrl+Alt+KeyK",
            "Ctrl+Alt+Shift+KeyK",
            "Alt+Ctrl+KeyK",
            "Shift+Ctrl+Alt+KeyK",
            "Ctrl+Alt",
        ] {
            reject(text);
        }
        assert!(Key::ALL.iter().all(|key| key.token() != "Escape"));
        assert!(Key::from_token("Escape").is_none());
    }

    #[test]
    fn identical_story_bindings_conflict() {
        let mut bindings = Keybindings::defaults();
        bindings.advance = Some(binding("KeyA"));
        bindings.fast_forward = Some(binding("KeyA"));
        let reason = bindings.validate().expect_err("相同触发必须冲突");
        assert!(reason.contains("前进"));
        assert!(reason.contains("快进"));
        assert!(reason.contains("相同"));
        assert!(bindings.to_json().is_err());
    }

    #[test]
    fn held_modifier_is_rejected_as_advance_prefix() {
        let mut bindings = Keybindings::defaults();
        bindings.advance = Some(binding("Ctrl+KeyK"));
        bindings.fast_forward = Some(binding("Ctrl"));
        let reason = bindings.validate().expect_err("按住前缀必须冲突");
        assert!(reason.contains("前进"));
        assert!(reason.contains("快进"));
        assert!(reason.contains("前缀"));
    }

    #[test]
    fn context_separated_bindings_are_allowed() {
        let mut bindings = Keybindings::defaults();
        bindings.advance = Some(binding("Space"));
        bindings.fast_forward = Some(binding("Ctrl"));
        bindings.skip_sequence = Some(binding("Space"));
        assert!(bindings.validate().is_ok());

        let mut bindings = Keybindings::defaults();
        bindings.fast_forward = Some(binding("Ctrl"));
        bindings.skip_sequence = Some(binding("Ctrl+KeyK"));
        assert!(bindings.validate().is_ok());

        let mut bindings = Keybindings::defaults();
        bindings.advance = Some(binding("Enter"));
        bindings.fast_forward = Some(binding("Space"));
        bindings.skip_sequence = Some(binding("Space"));
        assert!(bindings.validate().is_ok());
    }

    #[test]
    fn hold_is_only_valid_for_fast_forward() {
        for action in [Action::Advance, Action::SkipSequence] {
            let mut bindings = Keybindings::defaults();
            bindings.set(action, Some(binding("Ctrl")));
            let reason = bindings.validate().expect_err("非快进动作不能按住");
            assert!(reason.contains(action.label()));
            assert!(reason.contains("按住"));
        }
        let mut bindings = Keybindings::defaults();
        bindings.fast_forward = Some(binding("Alt"));
        assert!(bindings.validate().is_ok());
    }

    #[test]
    fn swapping_two_bindings_is_valid() {
        let mut bindings = Keybindings::defaults();
        bindings.advance = Some(binding("Enter"));
        bindings.fast_forward = Some(binding("Tab"));
        bindings.skip_sequence = Some(binding("Space"));
        bindings.validate().expect("交换前应合法");

        let advance = bindings.advance;
        let fast_forward = bindings.fast_forward;
        bindings.advance = fast_forward;
        bindings.fast_forward = advance;
        bindings.validate().expect("整体交换后应合法");
        assert_eq!(bindings.advance, Some(binding("Tab")));
        assert_eq!(bindings.fast_forward, Some(binding("Enter")));
    }

    #[test]
    fn null_disables_a_binding() {
        let mut bindings = Keybindings::defaults();
        bindings.set(Action::Advance, None);
        assert!(bindings.validate().is_ok());
        assert_eq!(bindings.display(Action::Advance), "未绑定");
        let json = bindings.to_json().expect("禁用绑定应可序列化");
        assert!(json.contains("\"advance\":null"));
        assert_eq!(
            Keybindings::from_json(&json).expect("禁用绑定应可反序列化"),
            bindings
        );
    }

    #[test]
    fn display_is_readable_not_storage() {
        assert_eq!(Key::Space.display(), "空格");
        assert_eq!(Key::KeyA.display(), "A");
        assert_eq!(Key::Digit0.display(), "0");
        assert_eq!(Key::NumpadEnter.display(), "小键盘回车");
        assert!(Key::F1.display().contains("F1"));

        let space = binding("Space");
        assert_eq!(space.display(), "空格");
        assert_ne!(space.display(), space.canonical());
        assert!(Binding::parse(&space.display()).is_err());

        let combo = binding("Ctrl+Shift+KeyK");
        assert_eq!(combo.display(), "Ctrl+Shift+K");
        assert_ne!(combo.display(), combo.canonical());
        assert!(Binding::parse(&combo.display()).is_err());

        assert_eq!(
            Keybindings::defaults().display(Action::FastForward),
            "按住 \u{2068}Ctrl\u{2069}"
        );
        assert_eq!(Keybindings::defaults().display(Action::Advance), "空格");
    }

    #[test]
    fn bindings_json_is_exact_and_strict() {
        let bindings = example_bindings();
        let json = bindings.to_json().expect("示例映射应可序列化");
        assert_eq!(
            json,
            r#"{"advance":"Space","fast_forward":"Ctrl","skip_sequence":null}"#
        );
        assert_eq!(
            Keybindings::from_json(&json).expect("示例映射应可反序列化"),
            bindings
        );

        let all_null = r#"{"advance":null,"fast_forward":null,"skip_sequence":null}"#;
        assert_eq!(
            Keybindings::from_json(all_null).expect("三个 null 合法"),
            Keybindings {
                advance: None,
                fast_forward: None,
                skip_sequence: None,
            }
        );

        let missing = Keybindings::from_json(r#"{"advance":null,"fast_forward":null}"#)
            .expect_err("缺少动作字段必须拒绝");
        assert!(missing.contains("缺少字段"));

        let unknown = Keybindings::from_json(
            r#"{"advance":null,"fast_forward":null,"skip_sequence":null,"extra":null}"#,
        )
        .expect_err("未知字段必须拒绝");
        assert!(unknown.contains("未知字段"));

        let duplicate = Keybindings::from_json(
            r#"{"advance":null,"advance":"Space","fast_forward":null,"skip_sequence":null}"#,
        )
        .expect_err("重复字段必须拒绝");
        assert!(duplicate.contains("重复字段"));

        for text in [
            r#"{"advance":1,"fast_forward":null,"skip_sequence":null}"#,
            r#"{"advance":true,"fast_forward":null,"skip_sequence":null}"#,
            r#"{"advance":[],"fast_forward":null,"skip_sequence":null}"#,
            r#"{"advance":{},"fast_forward":null,"skip_sequence":null}"#,
            r#"{"advance":null,"fast_forward":null,"skip_sequence":null} trailing"#,
            r#"{"advance":"Ctrl+Alt+KeyK","fast_forward":null,"skip_sequence":null}"#,
            r#"{"advance":"Ctrl","fast_forward":null,"skip_sequence":null}"#,
            r#"{"advance":"Space","fast_forward":"Space","skip_sequence":null}"#,
            "null",
            "[]",
        ] {
            assert!(
                Keybindings::from_json(text).is_err(),
                "应拒绝快捷键 JSON：{text}"
            );
        }
    }

    #[test]
    fn keybindings_file_round_trips_exactly() {
        let bindings = example_bindings();
        let bytes = KeybindingsFile::to_bytes("story", &bindings).expect("应可写入偏好");
        assert!(bytes.len() <= 16 * 1024);
        assert_eq!(
            String::from_utf8(bytes.clone()).expect("JSON 是 UTF-8"),
            r#"{"schema_version":1,"story_id":"story","bindings":{"advance":"Space","fast_forward":"Ctrl","skip_sequence":null}}"#
        );
        assert_eq!(
            KeybindingsFile::load_bytes(&bytes, "story").expect("应可读取偏好"),
            bindings
        );

        let mismatch =
            KeybindingsFile::load_bytes(&bytes, "other").expect_err("story_id 必须精确匹配");
        assert!(mismatch.contains("story_id"));
    }

    #[test]
    fn keybindings_file_is_strict() {
        let bindings = example_bindings();
        let valid = String::from_utf8(
            KeybindingsFile::to_bytes("story", &bindings).expect("示例偏好应可写入"),
        )
        .expect("JSON 是 UTF-8");

        for text in [
            valid.replace("\"schema_version\":1", "\"schema_version\":2"),
            valid.replace("\"story_id\":\"story\"", "\"story_id\":\"other\""),
            valid.replace("\"schema_version\":1", "\"schema_version\":\"1\""),
            valid.replace("\"story_id\":\"story\"", "\"story_id\":1"),
            valid.replace(
                "\"bindings\":",
                "\"extra\":null,\"bindings\":",
            ),
            valid.replace(
                "\"schema_version\":1,",
                "\"schema_version\":1,\"schema_version\":1,",
            ),
            valid.replace(
                "{\"schema_version\":1,\"story_id\":\"story\",\"bindings\":{\"advance\":\"Space\",\"fast_forward\":\"Ctrl\",\"skip_sequence\":null}}",
                "{\"schema_version\":1,\"story_id\":\"story\"}",
            ),
            valid.replace("\"schema_version\":1,", ""),
            valid.replace("\"story_id\":\"story\",", ""),
            valid.replace("\"bindings\":{", "\"bindings\":null,\"unused\":{"),
            valid.replace(
                "\"advance\":\"Space\",",
                "\"advance\":\"Space\",\"unknown\":null,",
            ),
            valid.replace("\"advance\":\"Space\"", "\"advance\":1"),
            valid.replace("\"advance\":\"Space\"", "\"advance\":\"Ctrl+Alt+KeyK\""),
            valid.replace(
                "\"fast_forward\":\"Ctrl\"",
                "\"fast_forward\":\"Space\"",
            ),
            valid.replace(
                "\"advance\":\"Space\"",
                "\"advance\":\"Space\",\"advance\":\"Enter\"",
            ),
        ] {
            assert!(
                KeybindingsFile::load_bytes(text.as_bytes(), "story").is_err(),
                "应拒绝偏好文件：{text}"
            );
        }

        let oversized = vec![b' '; 16 * 1024 + 1];
        let reason =
            KeybindingsFile::load_bytes(&oversized, "story").expect_err("超过 16KiB 必须拒绝");
        assert!(reason.contains("16KiB"));

        let oversized_story = "x".repeat(16 * 1024);
        let reason = KeybindingsFile::to_bytes(&oversized_story, &bindings)
            .expect_err("序列化超过 16KiB 必须拒绝");
        assert!(reason.contains("16KiB"));

        let empty = KeybindingsFile::load_bytes(&[], "story").expect_err("空文件必须拒绝");
        assert!(!empty.is_empty());
    }

    #[test]
    fn file_size_limit_is_inclusive_at_16_kib() {
        let bindings = example_bindings();
        let overhead = KeybindingsFile::to_bytes("", &bindings)
            .expect("空 story_id 也应可序列化")
            .len();
        let padding = 16 * 1024 - overhead;
        let story_id = "x".repeat(padding);
        let bytes = KeybindingsFile::to_bytes(&story_id, &bindings).expect("恰好 16KiB 应可写入");
        assert_eq!(bytes.len(), 16 * 1024);
        assert_eq!(
            KeybindingsFile::load_bytes(&bytes, &story_id).expect("恰好 16KiB 应可读取"),
            bindings
        );
    }

    #[test]
    fn validate_rechecks_constructed_bindings() {
        let reserved = Binding {
            trigger: Trigger::Key {
                modifiers: Modifiers {
                    ctrl: true,
                    alt: true,
                    shift: false,
                },
                key: Key::KeyA,
            },
        };
        let mut bindings = Keybindings::defaults();
        bindings.advance = Some(reserved);
        assert!(bindings.validate().is_err());

        let alt_f4 = Binding {
            trigger: Trigger::Key {
                modifiers: Modifiers {
                    ctrl: false,
                    alt: true,
                    shift: false,
                },
                key: Key::F4,
            },
        };
        let mut bindings = Keybindings::defaults();
        bindings.skip_sequence = Some(alt_f4);
        assert!(bindings.validate().is_err());
    }
}
