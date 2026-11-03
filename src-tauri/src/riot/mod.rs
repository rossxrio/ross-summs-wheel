//! Data from Riot.
//!
//! - `ddragon`: static data for the current patch (spell cooldowns, icons,
//!   which items give summoner spell haste) from the Data Dragon CDN.
//! - `live_client`: the running match (enemy champions, roles, spells, items)
//!   from the game client's local Live Client Data API.

pub mod ddragon;
pub mod live_client;
