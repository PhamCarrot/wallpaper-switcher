//! Narrow source presentation data; no config IO, daemon, mutation, or worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub logo: bool,
}

impl Mods {
    pub const NONE: Self = Self {
        ctrl: false,
        alt: false,
        shift: false,
        logo: false,
    };

    pub const fn new(ctrl: bool, alt: bool, shift: bool) -> Self {
        Self {
            ctrl,
            alt,
            shift,
            logo: false,
        }
    }

    fn label(self, output: &mut String) {
        if self.ctrl {
            output.push_str("Ctrl+");
        }
        if self.alt {
            output.push_str("Alt+");
        }
        if self.shift {
            output.push_str("Shift+");
        }
        if self.logo {
            output.push_str("Win+");
        }
    }

    fn config(self, output: &mut String) {
        if self.ctrl {
            output.push_str("ctrl+");
        }
        if self.alt {
            output.push_str("alt+");
        }
        if self.shift {
            output.push_str("shift+");
        }
        if self.logo {
            output.push_str("win+");
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyId {
    Char(String),
    Left,
    Right,
    Up,
    Down,
    Enter,
    Tab,
    Home,
    End,
    PageUp,
    PageDown,
    Escape,
    Delete,
    Function(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySpec {
    pub mods: Mods,
    pub id: KeyId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseSpec {
    pub mods: Mods,
    pub button: MouseButton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelDirection {
    Up,
    Down,
}

impl WheelDirection {
    pub fn from_delta(delta: f32) -> Option<Self> {
        (delta.is_finite() && delta != 0.0).then(|| if delta > 0.0 { Self::Up } else { Self::Down })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WheelSpec {
    pub mods: Mods,
    pub direction: WheelDirection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    Key(KeySpec),
    Mouse(MouseSpec),
    Wheel(WheelSpec),
}

pub const UNBOUND: &str = "none";

pub const SLOTS: usize = 2;

impl Trigger {
    pub fn parse(text: &str) -> Option<Self> {
        let normalized = text.trim().to_lowercase();
        if normalized.is_empty() {
            return None;
        }
        let tokens: Vec<&str> = if normalized == "+" {
            vec!["+"]
        } else if let Some(head) = normalized.strip_suffix("++") {
            let mut parts: Vec<&str> = head.split('+').map(str::trim).collect();
            parts.push("+");
            parts
        } else {
            normalized.split('+').map(str::trim).collect()
        };
        let (last, modifiers) = tokens.split_last()?;
        let mut mods = Mods::NONE;
        for token in modifiers {
            match *token {
                "shift" => mods.shift = true,
                "ctrl" | "control" => mods.ctrl = true,
                "alt" => mods.alt = true,
                "win" | "windows" | "super" => mods.logo = true,
                "hyper" => {
                    mods.ctrl = true;
                    mods.alt = true;
                    mods.shift = true;
                    mods.logo = true;
                }
                _ => return None,
            }
        }
        let button = match *last {
            "click" | "left-click" => Some(MouseButton::Left),
            "right-click" => Some(MouseButton::Right),
            "middle-click" => Some(MouseButton::Middle),
            _ => None,
        };
        if let Some(button) = button {
            return Some(Self::Mouse(MouseSpec { mods, button }));
        }
        let wheel = match *last {
            "wheel-up" => Some(WheelDirection::Up),
            "wheel-down" => Some(WheelDirection::Down),
            _ => None,
        };
        if let Some(direction) = wheel {
            return Some(Self::Wheel(WheelSpec { mods, direction }));
        }
        let id = match *last {
            "left" => KeyId::Left,
            "right" => KeyId::Right,
            "up" => KeyId::Up,
            "down" => KeyId::Down,
            "enter" | "return" => KeyId::Enter,
            "tab" => KeyId::Tab,
            "home" => KeyId::Home,
            "end" => KeyId::End,
            "pageup" | "page_up" | "pgup" => KeyId::PageUp,
            "pagedown" | "page_down" | "pgdn" => KeyId::PageDown,
            "escape" | "esc" => KeyId::Escape,
            "delete" | "del" => KeyId::Delete,
            key if key
                .strip_prefix('f')
                .and_then(|number| number.parse::<u8>().ok())
                .is_some_and(|number| {
                    (1..=12).contains(&number) && key[1..] == number.to_string()
                }) =>
            {
                KeyId::Function(key[1..].parse().expect("validated F1-F12 number"))
            }
            "space" => KeyId::Char(" ".to_string()),
            key if key.chars().count() == 1 => KeyId::Char(key.to_string()),
            _ => return None,
        };
        Some(Self::Key(KeySpec { mods, id }))
    }

    pub fn label(&self) -> String {
        let mut output = String::new();
        match self {
            Self::Key(spec) => {
                spec.mods.label(&mut output);
                match &spec.id {
                    KeyId::Left => output.push_str("Left"),
                    KeyId::Right => output.push_str("Right"),
                    KeyId::Up => output.push_str("Up"),
                    KeyId::Down => output.push_str("Down"),
                    KeyId::Enter => output.push_str("Enter"),
                    KeyId::Tab => output.push_str("Tab"),
                    KeyId::Home => output.push_str("Home"),
                    KeyId::End => output.push_str("End"),
                    KeyId::PageUp => output.push_str("Page Up"),
                    KeyId::PageDown => output.push_str("Page Down"),
                    KeyId::Escape => output.push_str("Escape"),
                    KeyId::Delete => output.push_str("Delete"),
                    KeyId::Function(number) => output.push_str(&format!("F{number}")),
                    KeyId::Char(character) if character == " " => output.push_str("Space"),
                    KeyId::Char(character) => output.push_str(&character.to_uppercase()),
                }
            }
            Self::Mouse(spec) => {
                spec.mods.label(&mut output);
                output.push_str(match spec.button {
                    MouseButton::Left => "Click",
                    MouseButton::Right => "Right-click",
                    MouseButton::Middle => "Middle-click",
                });
            }
            Self::Wheel(spec) => {
                spec.mods.label(&mut output);
                output.push_str(match spec.direction {
                    WheelDirection::Up => "Wheel up",
                    WheelDirection::Down => "Wheel down",
                });
            }
        }
        output
    }

    pub fn config(&self) -> String {
        let mut output = String::new();
        match self {
            Self::Key(spec) => {
                spec.mods.config(&mut output);
                match &spec.id {
                    KeyId::Left => output.push_str("left"),
                    KeyId::Right => output.push_str("right"),
                    KeyId::Up => output.push_str("up"),
                    KeyId::Down => output.push_str("down"),
                    KeyId::Enter => output.push_str("enter"),
                    KeyId::Tab => output.push_str("tab"),
                    KeyId::Home => output.push_str("home"),
                    KeyId::End => output.push_str("end"),
                    KeyId::PageUp => output.push_str("pageup"),
                    KeyId::PageDown => output.push_str("pagedown"),
                    KeyId::Escape => output.push_str("escape"),
                    KeyId::Delete => output.push_str("delete"),
                    KeyId::Function(number) => output.push_str(&format!("f{number}")),
                    KeyId::Char(character) if character == " " => output.push_str("space"),
                    KeyId::Char(character) => output.push_str(character),
                }
            }
            Self::Mouse(spec) => {
                spec.mods.config(&mut output);
                output.push_str(match spec.button {
                    MouseButton::Left => "click",
                    MouseButton::Right => "right-click",
                    MouseButton::Middle => "middle-click",
                });
            }
            Self::Wheel(spec) => {
                spec.mods.config(&mut output);
                output.push_str(match spec.direction {
                    WheelDirection::Up => "wheel-up",
                    WheelDirection::Down => "wheel-down",
                });
            }
        }
        output
    }

    pub const fn is_mouse(&self) -> bool {
        matches!(self, Self::Mouse(_) | Self::Wheel(_))
    }
}

pub fn parse_binding(text: &str) -> Option<Vec<Trigger>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.eq_ignore_ascii_case(UNBOUND) {
        return Some(Vec::new());
    }
    trimmed.split(',').map(Trigger::parse).collect()
}

pub fn binding_config(triggers: &[Trigger]) -> String {
    if triggers.is_empty() {
        return String::from(UNBOUND);
    }
    triggers
        .iter()
        .map(Trigger::config)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn binding_label(triggers: &[Trigger]) -> String {
    triggers
        .iter()
        .map(Trigger::label)
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

pub fn slot_triggers(triggers: &[Trigger], slot: usize) -> &[Trigger] {
    let (primary, secondary) = triggers.split_at(triggers.len().min(1));
    if slot == 0 {
        primary
    } else {
        secondary
    }
}

pub fn with_slot(triggers: &[Trigger], slot: usize, replacement: Option<Trigger>) -> Vec<Trigger> {
    let (primary, secondary) = triggers.split_at(triggers.len().min(1));
    let combined: Vec<Trigger> = if slot == 0 {
        replacement
            .into_iter()
            .chain(secondary.iter().cloned())
            .collect()
    } else {
        primary.iter().cloned().chain(replacement).collect()
    };
    let mut unique = Vec::with_capacity(combined.len());
    for trigger in combined {
        if !unique.contains(&trigger) {
            unique.push(trigger);
        }
    }
    unique
}
