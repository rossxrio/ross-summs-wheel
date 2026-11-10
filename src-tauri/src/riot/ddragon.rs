//! Summoner spell names, icons and cooldowns, plus which items give summoner
//! spell haste, from Riot's Data Dragon CDN, so they stay correct across patches.

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, time::Duration};

const DDRAGON: &str = "https://ddragon.leagueoflegends.com";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpellData {
    version: Option<String>,
    spells: HashMap<String, SpellInfo>,
    /// Item id -> summoner spell haste it grants (e.g. Ionian Boots of Lucidity).
    item_haste: HashMap<String, f64>,
}

#[derive(Serialize)]
pub struct SpellInfo {
    id: String,
    name: String,
    cooldown: f64,
    image: String,
}

#[derive(Deserialize)]
struct SummonerJson {
    data: HashMap<String, RawSpell>,
}

#[derive(Deserialize)]
struct RawSpell {
    id: String,
    name: String,
    cooldown: Vec<f64>,
    image: RawImage,
}

#[derive(Deserialize)]
struct RawImage {
    full: String,
}

#[derive(Deserialize)]
struct ItemJson {
    data: HashMap<String, RawItem>,
}

#[derive(Deserialize)]
struct RawItem {
    #[serde(default)]
    description: String,
}

#[tauri::command]
pub async fn get_spell_data() -> Result<SpellData, String> {
    fetch().await.map_err(|e| e.to_string())
}

// Send + Sync so the command's future can run on Tauri's async runtime.
async fn fetch() -> Result<SpellData, Box<dyn std::error::Error + Send + Sync>> {
    // item.json is several hundred KB: allow a slow connection time to fetch it.
    let client = reqwest::Client::builder().timeout(Duration::from_secs(20)).build()?;

    let versions: Vec<String> = client
        .get(format!("{DDRAGON}/api/versions.json"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let version = versions.into_iter().next().ok_or("empty Data Dragon version list")?;

    let summoners: SummonerJson = client
        .get(format!("{DDRAGON}/cdn/{version}/data/en_US/summoner.json"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let items: ItemJson = client
        .get(format!("{DDRAGON}/cdn/{version}/data/en_US/item.json"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let item_haste = items
        .data
        .into_iter()
        .filter_map(|(id, item)| summoner_haste(&item.description).map(|h| (id, h)))
        .collect();

    let spells = summoners
        .data
        .into_values()
        .map(|s| {
            let info = SpellInfo {
                cooldown: s.cooldown.first().copied().unwrap_or(0.0),
                image: s.image.full,
                name: s.name,
                id: s.id.clone(),
            };
            (s.id, info)
        })
        .collect();

    Ok(SpellData {
        version: Some(version),
        spells,
        item_haste,
    })
}

/// Data Dragon has no stat field for summoner spell haste; it only appears in
/// the description, e.g. `<passive>Ionian Insight</passive><br>Gain 10 Summoner Spell Haste.`
/// Reads the number right before that phrase, ignoring markup.
fn summoner_haste(description: &str) -> Option<f64> {
    let end = description.find("Summoner Spell Haste")?;
    let mut text = String::new();
    let mut in_tag = false;
    for c in description[..end].chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    let text = text.trim_end();
    let start = text
        .rfind(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map_or(0, |i| i + 1);
    text[start..].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::summoner_haste;

    #[test]
    fn reads_haste_from_item_descriptions() {
        let ionian = "<mainText><stats><attention>10</attention> Ability Haste<br><attention>45</attention> Move Speed</stats><br><br><passive>Ionian Insight</passive><br>Gain 10 Summoner Spell Haste.<br><br></mainText>";
        assert_eq!(summoner_haste(ionian), Some(10.0));
        assert_eq!(
            summoner_haste("Gain <attention>20</attention> Summoner Spell Haste."),
            Some(20.0)
        );
        assert_eq!(summoner_haste("<stats>40 Ability Haste</stats>"), None);
    }
}
