//! Polls League's Live Client Data API (served by the game client on
//! https://127.0.0.1:2999 while a match is running) for the enemy champions,
//! roles and summoner spells.

use serde::{Deserialize, Serialize};
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, Manager, State};

const BASE: &str = "https://127.0.0.1:2999/liveclientdata";
const POLL_EVERY: Duration = Duration::from_secs(3);
/// Consecutive failed polls before we decide the game is over. Stops one slow
/// response from wiping the roster mid-game.
const MISSES_BEFORE_IDLE: u32 = 3;

#[derive(Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameState {
    in_game: bool,
    /// 11 = Summoner's Rift, 12 = Howling Abyss (ARAM, which has a summoner haste aura).
    map_number: Option<u32>,
    enemies: Vec<Enemy>,
}

#[derive(Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enemy {
    role: Option<String>,
    champion: String,
    champion_id: String,
    spells: [String; 2],
    /// Champion level: Unleashed Teleport's cooldown scales with it.
    level: u32,
    /// Item ids. Boots like Ionian Boots of Lucidity give summoner spell haste.
    items: Vec<u32>,
    /// Primary and secondary rune tree ids. Only the trees are visible for
    /// enemies, not minor runes such as Cosmic Insight.
    rune_trees: Vec<u32>,
}

#[derive(Default)]
pub struct LiveState(Mutex<GameState>);

#[tauri::command]
pub fn get_game_state(state: State<'_, LiveState>) -> GameState {
    state.0.lock().unwrap().clone()
}

pub fn spawn_poller(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let client = reqwest::Client::builder()
            // The game serves a self-signed certificate, and only on localhost.
            .danger_accept_invalid_certs(true)
            .timeout(Duration::from_secs(2))
            .build()
            .expect("http client");

        let mut misses = 0;
        loop {
            let next = match fetch(&client).await {
                Ok(state) => {
                    misses = 0;
                    Some(state)
                }
                Err(_) => {
                    misses += 1;
                    (misses >= MISSES_BEFORE_IDLE).then(GameState::default)
                }
            };

            if let Some(next) = next {
                let changed = {
                    let live = app.state::<LiveState>();
                    let mut current = live.0.lock().unwrap();
                    let changed = *current != next;
                    *current = next.clone();
                    changed
                };
                if changed {
                    let _ = app.emit("game-state", next);
                }
            }
            tokio::time::sleep(POLL_EVERY).await;
        }
    });
}

async fn fetch(client: &reqwest::Client) -> reqwest::Result<GameState> {
    let me: String = client
        .get(format!("{BASE}/activeplayername"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let players: Vec<Player> = client
        .get(format!("{BASE}/playerlist"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    // Spectating, or the player list isn't populated yet: nothing to track.
    let Some(my_team) = players.iter().find(|p| p.is(&me)).map(|p| p.team.as_str()) else {
        return Ok(GameState::default());
    };

    let enemies = players
        .iter()
        .filter(|p| p.team != my_team)
        .map(Player::to_enemy)
        .collect();

    // Not essential: a failure here just means no map-specific haste.
    let map_number = match client.get(format!("{BASE}/gamestats")).send().await {
        Ok(res) => res.json::<GameStats>().await.ok().map(|g| g.map_number),
        Err(_) => None,
    };
    Ok(GameState {
        in_game: true,
        map_number,
        enemies,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameStats {
    map_number: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Player {
    champion_name: String,
    /// "game_character_displayname_MonkeyKing": carries the Data Dragon id.
    #[serde(default)]
    raw_champion_name: String,
    /// "ORDER" or "CHAOS".
    team: String,
    /// "TOP" | "JUNGLE" | "MIDDLE" | "BOTTOM" | "UTILITY", or "" outside draft/ranked.
    #[serde(default)]
    position: String,
    #[serde(default)]
    riot_id: String,
    #[serde(default)]
    riot_id_game_name: String,
    #[serde(default)]
    summoner_name: String,
    summoner_spells: SummonerSpells,
    #[serde(default)]
    level: u32,
    #[serde(default)]
    items: Vec<RawItem>,
    #[serde(default)]
    runes: Option<RawRunes>,
}

#[derive(Deserialize)]
struct RawItem {
    #[serde(rename = "itemID")]
    item_id: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRunes {
    primary_rune_tree: Option<RawRuneTree>,
    secondary_rune_tree: Option<RawRuneTree>,
}

#[derive(Deserialize)]
struct RawRuneTree {
    id: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummonerSpells {
    summoner_spell_one: RawSpell,
    summoner_spell_two: RawSpell,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSpell {
    #[serde(default)]
    display_name: String,
    /// "GeneratedTip_SummonerSpell_SummonerFlash_DisplayName": locale-independent id.
    #[serde(default)]
    raw_display_name: String,
}

impl Player {
    /// `activeplayername` returns "Name#TAG". Older clients used plain summoner names.
    fn is(&self, me: &str) -> bool {
        let game_name = me.split('#').next().unwrap_or(me);
        [&self.riot_id, &self.summoner_name]
            .iter()
            .any(|n| !n.is_empty() && *n == me)
            || (!self.riot_id_game_name.is_empty() && self.riot_id_game_name == game_name)
    }

    fn to_enemy(&self) -> Enemy {
        let champion_id = self
            .raw_champion_name
            .strip_prefix("game_character_displayname_")
            .map(str::to_string)
            .unwrap_or_else(|| self.champion_name.chars().filter(|c| c.is_alphanumeric()).collect());
        Enemy {
            role: (!self.position.is_empty()).then(|| self.position.to_uppercase()),
            champion: self.champion_name.clone(),
            champion_id,
            spells: [
                self.summoner_spells.summoner_spell_one.id(),
                self.summoner_spells.summoner_spell_two.id(),
            ],
            level: self.level,
            items: self.items.iter().map(|i| i.item_id).collect(),
            rune_trees: self
                .runes
                .iter()
                .flat_map(|r| [&r.primary_rune_tree, &r.secondary_rune_tree])
                .flatten()
                .map(|t| t.id)
                .collect(),
        }
    }
}

impl RawSpell {
    fn id(&self) -> String {
        self.raw_display_name
            .strip_prefix("GeneratedTip_SummonerSpell_")
            .and_then(|s| s.strip_suffix("_DisplayName"))
            .map(str::to_string)
            .unwrap_or_else(|| self.display_name.clone())
    }
}
