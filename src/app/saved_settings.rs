//! Action and connection settings kept across restarts in `settings.json`.
//! The daily routine, language, Dream Realm limit, and Guild Manager access
//! have their own files.

use super::{ActionBattleOptions, PlayerApp};
use crate::afk_journey::arena::OpponentPosition;
use serde::{Deserialize, Serialize};

const FILE: &str = "settings.json";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct SavedBattle {
    suggested_formations: bool,
    current_team_first: bool,
    skip_manual_formations: bool,
    max_formations: u32,
    attempts_per_formation: u32,
}

impl Default for SavedBattle {
    fn default() -> Self {
        Self::from(ActionBattleOptions::default())
    }
}

impl From<ActionBattleOptions> for SavedBattle {
    fn from(options: ActionBattleOptions) -> Self {
        Self {
            suggested_formations: options.suggested_formations,
            current_team_first: options.current_team_first,
            skip_manual_formations: options.skip_manual_formations,
            max_formations: options.max_formations,
            attempts_per_formation: options.attempts_per_formation,
        }
    }
}

impl From<SavedBattle> for ActionBattleOptions {
    fn from(saved: SavedBattle) -> Self {
        Self {
            suggested_formations: saved.suggested_formations,
            current_team_first: saved.current_team_first,
            skip_manual_formations: saved.skip_manual_formations,
            max_formations: saved.max_formations.clamp(1, 10),
            attempts_per_formation: saved.attempts_per_formation.clamp(1, 10),
        }
    }
}

/// Everything the user sets in the action dialogs and the ADB path they
/// applied. A missing or unknown field keeps its default, so older files load.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(super) struct SavedSettings {
    afk_normal: bool,
    afk_season: bool,
    towers: [bool; 4],
    dura_sweep: bool,
    dream_spend_gold: bool,
    battle: [SavedBattle; 6],
    arena_attempts: u32,
    arena_claim_free_attempts: bool,
    supreme_attempts: u32,
    /// 0 left, 1 middle, 2 right.
    supreme_opponent: usize,
    guild_dream_days: u8,
    guild_supreme_arena: bool,
    guild_activeness: bool,
    guild_afk_stages: bool,
    /// Only a path the user applied; without one ADBHelper keeps finding ADB
    /// itself, also after an emulator update moved it.
    adb_path: Option<String>,
}

impl Default for SavedSettings {
    fn default() -> Self {
        let selection = crate::afk_journey::afk_stages::StageSelection::default();
        let arena = crate::afk_journey::arena::ArenaSettings::default();
        let supreme = crate::afk_journey::arena::SupremeSettings::default();
        let guild = crate::afk_journey::progression::scanner::GuildSettings::default();
        Self {
            afk_normal: selection.normal,
            afk_season: selection.season,
            towers: crate::afk_journey::legend_trial::TowerSelection::default().enabled,
            dura_sweep: true,
            dream_spend_gold: false,
            battle: [SavedBattle::default(); 6],
            arena_attempts: arena.attempts,
            arena_claim_free_attempts: arena.claim_free_attempts,
            supreme_attempts: supreme.attempts,
            supreme_opponent: opponent_index(supreme.opponent),
            guild_dream_days: guild.dream_days,
            guild_supreme_arena: guild.supreme_arena,
            guild_activeness: guild.activeness,
            guild_afk_stages: guild.afk_stages,
            adb_path: None,
        }
    }
}

fn opponent_index(opponent: OpponentPosition) -> usize {
    OpponentPosition::ALL
        .iter()
        .position(|&known| known == opponent)
        .unwrap_or(0)
}

impl SavedSettings {
    pub(super) fn from_app(app: &PlayerApp) -> Self {
        Self {
            afk_normal: app.stage_selection.normal,
            afk_season: app.stage_selection.season,
            towers: app.tower_selection.enabled,
            dura_sweep: app.dura_sweep_when_available,
            dream_spend_gold: app.dream_spend_gold,
            battle: app.battle_options.map(SavedBattle::from),
            arena_attempts: app.arena_settings.attempts,
            arena_claim_free_attempts: app.arena_settings.claim_free_attempts,
            supreme_attempts: app.supreme_settings.attempts,
            supreme_opponent: opponent_index(app.supreme_settings.opponent),
            guild_dream_days: app.guild_scan.dream_days,
            guild_supreme_arena: app.guild_scan.supreme_arena,
            guild_activeness: app.guild_scan.activeness,
            guild_afk_stages: app.guild_scan.afk_stages,
            adb_path: app.adb_path_applied.clone(),
        }
    }

    pub(super) fn apply(&self, app: &mut PlayerApp) {
        app.stage_selection.normal = self.afk_normal;
        app.stage_selection.season = self.afk_season;
        app.tower_selection.enabled = self.towers;
        app.dura_sweep_when_available = self.dura_sweep;
        app.dream_spend_gold = self.dream_spend_gold;
        app.battle_options = self.battle.map(ActionBattleOptions::from);
        app.arena_settings.attempts = self.arena_attempts;
        app.arena_settings.claim_free_attempts = self.arena_claim_free_attempts;
        app.supreme_settings.attempts = self.supreme_attempts;
        app.supreme_settings.opponent = OpponentPosition::ALL
            .get(self.supreme_opponent)
            .copied()
            .unwrap_or(OpponentPosition::ALL[0]);
        app.guild_scan.dream_days = self.guild_dream_days;
        app.guild_scan.supreme_arena = self.guild_supreme_arena;
        app.guild_scan.activeness = self.guild_activeness;
        app.guild_scan.afk_stages = self.guild_afk_stages;
        if let Some(path) = self
            .adb_path
            .as_deref()
            .filter(|path| !path.trim().is_empty())
        {
            std::env::set_var("ADB_PATH", path.trim());
            app.adb_path = path.trim().to_owned();
            app.adb_path_applied = Some(path.trim().to_owned());
        }
    }

    /// The saved settings, defaults when there is no file yet.
    pub(super) fn load() -> Result<Self, String> {
        let path = crate::settings::file(FILE).map_err(|e| e.to_string())?;
        match std::fs::read(path) {
            Ok(bytes) => Self::parse(&bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.to_string()),
        }
    }

    fn parse(bytes: &[u8]) -> Result<Self, String> {
        serde_json::from_slice(bytes).map_err(|e| e.to_string())
    }

    pub(super) fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        crate::settings::write(FILE, &bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_settings_come_back_after_a_restart() {
        let mut settings = SavedSettings {
            afk_season: false,
            towers: [true, false, true, false],
            dura_sweep: false,
            supreme_attempts: 3,
            supreme_opponent: 2,
            adb_path: Some("D:\\adb\\adb.exe".into()),
            ..SavedSettings::default()
        };
        settings.battle[4].max_formations = 7;
        settings.battle[4].skip_manual_formations = true;
        let bytes = serde_json::to_vec_pretty(&settings).unwrap();
        assert_eq!(SavedSettings::parse(&bytes).unwrap(), settings);
    }

    #[test]
    fn a_file_from_an_older_version_keeps_defaults_for_new_fields() {
        let settings = SavedSettings::parse(br#"{"dura_sweep": false}"#).unwrap();
        assert!(!settings.dura_sweep);
        assert_eq!(settings.battle, [SavedBattle::default(); 6]);
        assert_eq!(settings.adb_path, None);
        let partial = SavedSettings::parse(br#"{"battle": [{"max_formations": 4}]}"#);
        assert!(partial.is_err(), "the battle list always has six entries");
    }

    #[test]
    fn saved_battle_values_are_kept_in_range() {
        let saved = SavedBattle {
            max_formations: 0,
            attempts_per_formation: 99,
            ..SavedBattle::default()
        };
        let options = ActionBattleOptions::from(saved);
        assert_eq!(options.max_formations, 1);
        assert_eq!(options.attempts_per_formation, 10);
    }
}
