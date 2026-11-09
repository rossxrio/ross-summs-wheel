//! Quick binds: while the wheel hotkey is held, typing a short key sequence
//! (e.g. T then D) starts an enemy's spell timer without using the wheel.
//! Mouse buttons 4 and 5 can be part of a sequence too.

use crate::key_watch::Button;
use serde::{Deserialize, Serialize};

/// One bind as stored in config.json.
#[derive(Clone, Serialize, Deserialize)]
pub struct QuickBind {
    /// Riot position, e.g. "TOP".
    pub role: String,
    /// Base spell id, e.g. "SummonerFlash".
    pub spell: String,
    /// W3C key codes (or "Mouse4"/"Mouse5") in order, e.g. ["KeyT", "KeyD"].
    pub keys: Vec<String>,
}

/// Sent to the overlay as a `quick-bind` event when a sequence completes.
#[derive(Clone, Serialize)]
pub struct Target {
    pub role: String,
    pub spell: String,
}

/// Matches typed keys against the binds.
#[derive(Default)]
pub struct Matcher {
    binds: Vec<(Vec<Button>, Target)>,
    typed: Vec<Button>,
}

impl Matcher {
    pub fn new(binds: &[QuickBind]) -> Result<Self, String> {
        let binds = binds
            .iter()
            .map(|bind| {
                let keys = bind.keys.iter().map(|k| parse_key(k)).collect::<Result<Vec<_>, _>>()?;
                if keys.is_empty() {
                    return Err(format!("a {} bind has no keys", bind.role));
                }
                let target = Target {
                    role: bind.role.clone(),
                    spell: bind.spell.clone(),
                };
                Ok((keys, target))
            })
            .collect::<Result<_, String>>()?;
        Ok(Self { binds, typed: vec![] })
    }

    /// Every key used by some bind, once each.
    pub fn keys(&self) -> Vec<Button> {
        let mut keys: Vec<Button> = vec![];
        for key in self.binds.iter().flat_map(|(k, _)| k) {
            if !keys.contains(key) {
                keys.push(*key);
            }
        }
        keys
    }

    /// Forget what was typed (the hotkey was pressed again).
    pub fn reset(&mut self) {
        self.typed.clear();
    }

    /// Adds a key press; returns the bind it completes, if any.
    ///
    /// A key that doesn't continue any bind starts over from that key, so a
    /// mistyped first key doesn't need the hotkey to be pressed again.
    pub fn push(&mut self, key: Button) -> Option<Target> {
        self.typed.push(key);
        while !self.typed.is_empty() && !self.binds.iter().any(|(keys, _)| keys.starts_with(&self.typed)) {
            self.typed.remove(0);
        }
        let target = self.binds.iter().find(|(keys, _)| *keys == self.typed)?.1.clone();
        self.typed.clear();
        Some(target)
    }
}

fn parse_key(code: &str) -> Result<Button, String> {
    code.parse()
        .map_err(|_| format!("{code} can't be used in a quick bind"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::Code;

    const T: Button = Button::Key(Code::KeyT);
    const D: Button = Button::Key(Code::KeyD);
    const J: Button = Button::Key(Code::KeyJ);
    const M: Button = Button::Key(Code::KeyM);
    const X: Button = Button::Key(Code::KeyX);

    fn matcher(binds: &[(&str, &[&str])]) -> Matcher {
        let binds: Vec<QuickBind> = binds
            .iter()
            .map(|(role, keys)| QuickBind {
                role: role.to_string(),
                spell: "SummonerFlash".into(),
                keys: keys.iter().map(|k| k.to_string()).collect(),
            })
            .collect();
        Matcher::new(&binds).unwrap()
    }

    fn typed(m: &mut Matcher, keys: &[Button]) -> Vec<String> {
        keys.iter().filter_map(|k| m.push(*k)).map(|t| t.role).collect()
    }

    #[test]
    fn completes_a_sequence() {
        let mut m = matcher(&[("TOP", &["KeyT", "KeyD"]), ("JUNGLE", &["KeyJ", "KeyD"])]);
        assert_eq!(typed(&mut m, &[T]), Vec::<String>::new());
        assert_eq!(typed(&mut m, &[D]), vec!["TOP"]);
        // Binds chain within one hold.
        assert_eq!(typed(&mut m, &[J, D]), vec!["JUNGLE"]);
    }

    #[test]
    fn a_wrong_key_starts_over() {
        let mut m = matcher(&[("TOP", &["KeyT", "KeyD"]), ("JUNGLE", &["KeyJ", "KeyD"])]);
        // T then J: J begins the jungle bind instead of breaking everything.
        assert_eq!(typed(&mut m, &[T, J, D]), vec!["JUNGLE"]);
        assert_eq!(typed(&mut m, &[X, T, D]), vec!["TOP"]);
    }

    #[test]
    fn single_key_binds_and_reset() {
        let mut m = matcher(&[("MIDDLE", &["KeyM"]), ("TOP", &["KeyT", "KeyD"])]);
        assert_eq!(typed(&mut m, &[T, M]), vec!["MIDDLE"]);
        typed(&mut m, &[T]);
        m.reset();
        assert_eq!(typed(&mut m, &[D]), Vec::<String>::new());
        assert_eq!(m.keys(), vec![M, T, D]);
    }

    #[test]
    fn rejects_bad_keys() {
        let bind = |keys: &[&str]| QuickBind {
            role: "TOP".into(),
            spell: "SummonerFlash".into(),
            keys: keys.iter().map(|k| k.to_string()).collect(),
        };
        assert!(Matcher::new(&[bind(&[])]).is_err());
        assert!(Matcher::new(&[bind(&["Ctrl+KeyT"])]).is_err());
        assert!(Matcher::new(&[bind(&["NotAKey"])]).is_err());
        assert!(Matcher::new(&[bind(&["Mouse3"])]).is_err());
    }

    #[test]
    fn mouse_buttons_in_sequences() {
        let mut m = matcher(&[("TOP", &["Mouse4", "KeyD"]), ("JUNGLE", &["Mouse5"])]);
        assert_eq!(typed(&mut m, &[Button::Mouse4, D]), vec!["TOP"]);
        assert_eq!(typed(&mut m, &[Button::Mouse5]), vec!["JUNGLE"]);
        assert_eq!(m.keys(), vec![Button::Mouse4, D, Button::Mouse5]);
    }
}
