//! Quick binds: while the wheel hotkey is held, typing a short key sequence
//! (e.g. T then D) starts an enemy's spell timer without using the wheel.

use serde::{Deserialize, Serialize};
use tauri_plugin_global_shortcut::{Code, Shortcut};

/// One bind as stored in config.json.
#[derive(Clone, Serialize, Deserialize)]
pub struct QuickBind {
    /// Riot position, e.g. "TOP".
    pub role: String,
    /// Base spell id, e.g. "SummonerFlash".
    pub spell: String,
    /// W3C key codes typed in order, e.g. ["KeyT", "KeyD"].
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
    binds: Vec<(Vec<Code>, Target)>,
    typed: Vec<Code>,
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
    pub fn keys(&self) -> Vec<Code> {
        let mut keys: Vec<Code> = vec![];
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
    pub fn push(&mut self, key: Code) -> Option<Target> {
        self.typed.push(key);
        while !self.typed.is_empty() && !self.binds.iter().any(|(keys, _)| keys.starts_with(&self.typed)) {
            self.typed.remove(0);
        }
        let target = self.binds.iter().find(|(keys, _)| *keys == self.typed)?.1.clone();
        self.typed.clear();
        Some(target)
    }
}

pub fn parse_key(code: &str) -> Result<Code, String> {
    match code.parse::<Shortcut>() {
        Ok(shortcut) if shortcut.mods.is_empty() => Ok(shortcut.key),
        _ => Err(format!("{code} can't be used in a quick bind")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn typed(m: &mut Matcher, keys: &[Code]) -> Vec<String> {
        keys.iter().filter_map(|k| m.push(*k)).map(|t| t.role).collect()
    }

    #[test]
    fn completes_a_sequence() {
        let mut m = matcher(&[("TOP", &["KeyT", "KeyD"]), ("JUNGLE", &["KeyJ", "KeyD"])]);
        assert_eq!(typed(&mut m, &[Code::KeyT]), Vec::<String>::new());
        assert_eq!(typed(&mut m, &[Code::KeyD]), vec!["TOP"]);
        // Binds chain within one hold.
        assert_eq!(typed(&mut m, &[Code::KeyJ, Code::KeyD]), vec!["JUNGLE"]);
    }

    #[test]
    fn a_wrong_key_starts_over() {
        let mut m = matcher(&[("TOP", &["KeyT", "KeyD"]), ("JUNGLE", &["KeyJ", "KeyD"])]);
        // T then J: J begins the jungle bind instead of breaking everything.
        assert_eq!(typed(&mut m, &[Code::KeyT, Code::KeyJ, Code::KeyD]), vec!["JUNGLE"]);
        assert_eq!(typed(&mut m, &[Code::KeyX, Code::KeyT, Code::KeyD]), vec!["TOP"]);
    }

    #[test]
    fn single_key_binds_and_reset() {
        let mut m = matcher(&[("MIDDLE", &["KeyM"]), ("TOP", &["KeyT", "KeyD"])]);
        assert_eq!(typed(&mut m, &[Code::KeyT, Code::KeyM]), vec!["MIDDLE"]);
        typed(&mut m, &[Code::KeyT]);
        m.reset();
        assert_eq!(typed(&mut m, &[Code::KeyD]), Vec::<String>::new());
        assert_eq!(m.keys(), vec![Code::KeyM, Code::KeyT, Code::KeyD]);
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
    }
}
