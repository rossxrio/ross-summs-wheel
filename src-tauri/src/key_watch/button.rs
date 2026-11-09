//! What can be bound: a keyboard key or a mouse side button.
//!
//! Config stores keys as W3C key codes ("KeyT", "Backquote") and the side
//! buttons as "Mouse4" (back) and "Mouse5" (forward), the names the settings
//! page writes. The global-shortcut plugin only knows keys, so mouse buttons
//! are seen by the backup sources in key_watch alone.

use std::str::FromStr;
use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Button {
    Key(Code),
    /// Mouse button 4 (XButton1, usually "back").
    Mouse4,
    /// Mouse button 5 (XButton2, usually "forward").
    Mouse5,
}

impl Button {
    fn mouse(name: &str) -> Option<Self> {
        let name = name.trim();
        if name.eq_ignore_ascii_case("Mouse4") {
            Some(Self::Mouse4)
        } else if name.eq_ignore_ascii_case("Mouse5") {
            Some(Self::Mouse5)
        } else {
            None
        }
    }

    pub fn is_mouse(self) -> bool {
        !matches!(self, Self::Key(_))
    }
}

/// One key or button, no modifiers: "KeyT", "Mouse4".
impl FromStr for Button {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        if let Some(mouse) = Self::mouse(s) {
            return Ok(mouse);
        }
        match s.parse::<Shortcut>() {
            Ok(shortcut) if shortcut.mods.is_empty() => Ok(Self::Key(shortcut.key)),
            _ => Err(()),
        }
    }
}

/// The wheel hotkey: modifiers plus one key or button, e.g. "Backquote",
/// "Ctrl+KeyQ", "Mouse4", "Shift+Mouse5".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub mods: Modifiers,
    pub button: Button,
}

impl Hotkey {
    /// The same hotkey for the global-shortcut plugin; `None` for a mouse
    /// button, which only the backup sources can see.
    pub fn shortcut(self) -> Option<Shortcut> {
        match self.button {
            Button::Key(code) => Some(Shortcut::new(Some(self.mods), code)),
            _ => None,
        }
    }
}

impl FromStr for Hotkey {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        let (prefix, last) = match s.rsplit_once('+') {
            Some((prefix, last)) => (Some(prefix), last),
            None => (None, s),
        };
        let Some(button) = Button::mouse(last) else {
            let shortcut: Shortcut = s.parse().map_err(|_| ())?;
            return Ok(Self {
                mods: shortcut.mods,
                button: Button::Key(shortcut.key),
            });
        };
        // The plugin's parser reads the modifiers, given any key to go with them.
        let mods = match prefix {
            Some(prefix) => format!("{prefix}+KeyA").parse::<Shortcut>().map_err(|_| ())?.mods,
            None => Modifiers::empty(),
        };
        Ok(Self { mods, button })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_keys_and_mouse_buttons() {
        assert_eq!("KeyT".parse(), Ok(Button::Key(Code::KeyT)));
        assert_eq!("Mouse4".parse(), Ok(Button::Mouse4));
        assert_eq!("mouse5".parse(), Ok(Button::Mouse5));
        assert_eq!("Ctrl+KeyT".parse::<Button>(), Err(()));
        assert_eq!("Mouse3".parse::<Button>(), Err(()));
        assert_eq!("NotAKey".parse::<Button>(), Err(()));
    }

    #[test]
    fn parses_hotkeys() {
        let hotkey = |s: &str| s.parse::<Hotkey>().map(|h| (h.mods, h.button));
        assert_eq!(
            hotkey("Backquote"),
            Ok((Modifiers::empty(), Button::Key(Code::Backquote)))
        );
        assert_eq!(
            hotkey("Ctrl+Alt+KeyQ"),
            Ok((Modifiers::CONTROL | Modifiers::ALT, Button::Key(Code::KeyQ)))
        );
        assert_eq!(hotkey("Mouse4"), Ok((Modifiers::empty(), Button::Mouse4)));
        assert_eq!(hotkey("Shift+Mouse5"), Ok((Modifiers::SHIFT, Button::Mouse5)));
        assert_eq!(hotkey("Bogus+Mouse5"), Err(()));
        assert_eq!(hotkey("Mouse6"), Err(()));
    }

    #[test]
    fn only_keys_become_shortcuts() {
        let key: Hotkey = "Ctrl+KeyQ".parse().unwrap();
        assert_eq!(key.shortcut(), Some("Ctrl+KeyQ".parse().unwrap()));
        let mouse: Hotkey = "Ctrl+Mouse4".parse().unwrap();
        assert_eq!(mouse.shortcut(), None);
    }
}
