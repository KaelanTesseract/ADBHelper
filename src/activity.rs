use crate::localization::text;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GameMode {
    SeasonAfkStages,
    AfkStages,
    SeasonLegendTrial(LegendTower),
    UnionCampaign,
    DurasTrials,
    Arena,
    SupremeArena,
    AfkRewards,
    FreeHourglasses,
    DiamondHourglasses,
    StageRewards,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LegendTower {
    Lightbearer,
    Wilder,
    Graveborn,
    Mauler,
}

impl LegendTower {
    pub const ALL: [Self; 4] = [
        Self::Lightbearer,
        Self::Wilder,
        Self::Graveborn,
        Self::Mauler,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Lightbearer => "Lightbearer",
            Self::Wilder => "Wilder",
            Self::Graveborn => "Graveborn",
            Self::Mauler => "Mauler",
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Lightbearer => "lightbearer",
            Self::Wilder => "wilder",
            Self::Graveborn => "graveborn",
            Self::Mauler => "mauler",
        }
    }
}

impl GameMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::SeasonAfkStages => text("Season AFK Stages", "Season-AFK-Stages"),
            Self::AfkStages => text("AFK Stages", "AFK-Stages"),
            Self::SeasonLegendTrial(LegendTower::Lightbearer) => "Legend Trial · Lightbearer",
            Self::SeasonLegendTrial(LegendTower::Wilder) => "Legend Trial · Wilder",
            Self::SeasonLegendTrial(LegendTower::Graveborn) => "Legend Trial · Graveborn",
            Self::SeasonLegendTrial(LegendTower::Mauler) => "Legend Trial · Mauler",
            Self::UnionCampaign => "Union Campaign",
            Self::DurasTrials => "Dura’s Trials",
            Self::Arena => "Arena",
            Self::SupremeArena => "Supreme Arena",
            Self::AfkRewards => text("AFK rewards", "AFK-Belohnungen"),
            Self::FreeHourglasses => text("Free hourglasses", "Kostenlose Sanduhren"),
            Self::DiamondHourglasses => text("Diamond hourglasses", "Diamanten-Sanduhren"),
            Self::StageRewards => text("AFK stage rewards", "AFK-Stufenbelohnungen"),
        }
    }

    pub fn is_legend_trial(self) -> bool {
        matches!(self, Self::SeasonLegendTrial(_))
    }

    pub fn is_pvp(self) -> bool {
        matches!(self, Self::Arena | Self::SupremeArena)
    }

    pub fn is_collection(self) -> bool {
        matches!(
            self,
            Self::AfkRewards
                | Self::FreeHourglasses
                | Self::DiamondHourglasses
                | Self::StageRewards
        )
    }

    pub fn has_external_progression(self) -> bool {
        self.is_legend_trial() || matches!(self, Self::UnionCampaign | Self::DurasTrials)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum BattleOutcome {
    Won,
    Lost,
    Unclear,
}

#[derive(Clone, Copy, Debug)]
pub enum ActivityEvent {
    ModeOpening(GameMode),
    ModeEntered(GameMode),
    BattleStarted(GameMode),
    BattleFinished(GameMode, BattleOutcome),
    StageCleared(GameMode),
    Swept(GameMode, u32),
    Collected(GameMode, u32),
}

#[derive(Default, Debug)]
pub struct ModeStats {
    pub attempts: u64,
    pub cleared: u64,
    pub wins: u64,
    pub losses: u64,
    pub sweeps: u64,
    pub collected: u64,
    attempts_for_cleared: u64,
    battle_pending: bool,
    clear_pending: bool,
}

#[derive(Default)]
pub struct RunActivity {
    pub modes: BTreeMap<GameMode, ModeStats>,
    pub current_mode: Option<GameMode>,
    pub target: Option<GameMode>,
    pub stage_history: Vec<StagePoint>,
    last_entered_mode: Option<GameMode>,
    started: Option<Instant>,
    finished: Option<Instant>,
}

#[derive(Clone, Copy, Debug)]
pub struct StagePoint {
    pub elapsed: Duration,
    pub cleared: u64,
}

impl RunActivity {
    pub fn start(&mut self, target: GameMode, now: Instant) {
        *self = Self {
            target: Some(target),
            started: Some(now),
            ..Self::default()
        };
        self.stage_history.push(StagePoint {
            elapsed: Duration::ZERO,
            cleared: 0,
        });
    }

    pub fn is_running(&self) -> bool {
        self.started.is_some() && self.finished.is_none()
    }

    pub fn finish(&mut self, now: Instant) {
        if self.is_running() {
            self.finished = Some(now);
            if self.current_mode.is_none() {
                self.current_mode = self.last_entered_mode;
            }
        }
    }

    pub fn elapsed(&self, now: Instant) -> Duration {
        self.started
            .map(|start| {
                self.finished
                    .unwrap_or(now)
                    .saturating_duration_since(start)
            })
            .unwrap_or_default()
    }

    pub fn apply(&mut self, event: ActivityEvent) {
        self.apply_at(event, Instant::now());
    }

    fn apply_at(&mut self, event: ActivityEvent, now: Instant) {
        if !self.is_running() {
            return;
        }
        if let ActivityEvent::ModeOpening(mode) = event {
            self.target = Some(mode);
            self.current_mode = None;
            return;
        }
        if let ActivityEvent::ModeEntered(mode) = event {
            self.current_mode = Some(mode);
            self.last_entered_mode = Some(mode);
            self.modes.entry(mode).or_default();
            return;
        }
        let mode = match event {
            ActivityEvent::BattleStarted(mode)
            | ActivityEvent::BattleFinished(mode, _)
            | ActivityEvent::StageCleared(mode) => mode,
            ActivityEvent::Swept(mode, _) | ActivityEvent::Collected(mode, _) => mode,
            ActivityEvent::ModeEntered(_) | ActivityEvent::ModeOpening(_) => unreachable!(),
        };
        let stats = self.modes.entry(mode).or_default();
        let mut cleared = false;
        match event {
            ActivityEvent::Swept(_, count) => {
                stats.sweeps = stats.sweeps.saturating_add(u64::from(count));
            }
            ActivityEvent::Collected(_, count) => {
                stats.collected = stats.collected.saturating_add(u64::from(count));
            }
            ActivityEvent::BattleStarted(_) if !stats.battle_pending => {
                stats.attempts = stats.attempts.saturating_add(1);
                stats.battle_pending = true;
                stats.clear_pending = false;
            }
            ActivityEvent::BattleFinished(_, outcome) if stats.battle_pending => {
                stats.battle_pending = false;
                match outcome {
                    BattleOutcome::Won => {
                        stats.wins = stats.wins.saturating_add(1);
                        stats.clear_pending = !mode.is_pvp();
                        cleared = mode.is_pvp();
                    }
                    BattleOutcome::Lost => stats.losses = stats.losses.saturating_add(1),
                    BattleOutcome::Unclear => {}
                }
            }
            ActivityEvent::StageCleared(_) if stats.clear_pending => {
                stats.cleared = stats.cleared.saturating_add(1);
                stats.attempts_for_cleared = stats.attempts;
                stats.clear_pending = false;
                cleared = true;
            }
            _ => {}
        }
        if cleared {
            let total = self
                .modes
                .iter()
                .map(|(mode, s)| if mode.is_pvp() { s.wins } else { s.cleared })
                .sum();
            let elapsed = self.elapsed(now).max(
                self.stage_history
                    .last()
                    .map(|p| p.elapsed)
                    .unwrap_or_default(),
            );
            self.stage_history.push(StagePoint {
                elapsed,
                cleared: total,
            });
        }
    }

    pub fn outcomes(&self) -> (u64, u64) {
        self.modes.values().fold((0, 0), |(wins, losses), s| {
            (wins + s.wins, losses + s.losses)
        })
    }

    pub fn is_pvp_run(&self) -> bool {
        self.target.is_some_and(GameMode::is_pvp) || self.modes.keys().any(|mode| mode.is_pvp())
    }

    pub fn success_rate(&self) -> Option<f64> {
        let (wins, losses) = self.outcomes();
        (wins + losses > 0).then(|| 100.0 * wins as f64 / (wins + losses) as f64)
    }

    pub fn attempts_per_clear(&self) -> Option<f64> {
        let (attempts, cleared) = self.modes.values().fold((0, 0), |(a, c), s| {
            (a + s.attempts_for_cleared, c + s.cleared)
        });
        (cleared > 0).then(|| attempts as f64 / cleared as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pvp_victories_feed_graphs_without_creating_stage_clears() {
        for mode in [GameMode::Arena, GameMode::SupremeArena] {
            let mut run = RunActivity::default();
            run.start(mode, Instant::now());
            run.apply(ActivityEvent::ModeEntered(mode));
            run.apply(ActivityEvent::BattleStarted(mode));
            run.apply(ActivityEvent::BattleFinished(mode, BattleOutcome::Won));
            run.apply(ActivityEvent::StageCleared(mode));
            run.apply(ActivityEvent::BattleStarted(mode));
            run.apply(ActivityEvent::BattleFinished(mode, BattleOutcome::Unclear));
            assert_eq!(run.modes[&mode].attempts, 2);
            assert_eq!(run.modes[&mode].wins, 1);
            assert_eq!(run.modes[&mode].cleared, 0);
            assert_eq!(run.stage_history.last().unwrap().cleared, 1);
            assert!(run.is_pvp_run());
            assert_eq!(run.attempts_per_clear(), None);
        }
    }
    const SEASON: GameMode = GameMode::SeasonAfkStages;

    #[test]
    fn collection_counts_do_not_become_battles_clears_or_wins() {
        let mut run = RunActivity::default();
        run.start(GameMode::AfkRewards, Instant::now());
        run.apply(ActivityEvent::ModeEntered(GameMode::AfkRewards));
        run.apply(ActivityEvent::Collected(GameMode::AfkRewards, 2));
        run.apply(ActivityEvent::ModeEntered(GameMode::FreeHourglasses));
        run.apply(ActivityEvent::Collected(GameMode::FreeHourglasses, 3));
        assert_eq!(run.modes[&GameMode::AfkRewards].collected, 2);
        assert_eq!(run.modes[&GameMode::FreeHourglasses].collected, 3);
        assert_eq!(run.outcomes(), (0, 0));
        assert_eq!(run.success_rate(), None);
        assert_eq!(run.attempts_per_clear(), None);
        assert_eq!(run.stage_history.len(), 1);
    }

    #[test]
    fn counters_survive_switches_and_results_stay_with_their_mode() {
        let mut run = RunActivity::default();
        run.start(SEASON, Instant::now());
        assert_eq!(run.current_mode, None);
        run.apply(ActivityEvent::ModeEntered(SEASON));
        run.apply(ActivityEvent::BattleStarted(SEASON));
        run.apply(ActivityEvent::ModeOpening(GameMode::AfkStages));
        assert_eq!(run.current_mode, None);
        assert_eq!(run.target, Some(GameMode::AfkStages));
        assert_eq!(run.modes[&SEASON].attempts, 1);
        run.apply(ActivityEvent::ModeEntered(GameMode::AfkStages));
        run.apply(ActivityEvent::BattleFinished(SEASON, BattleOutcome::Won));
        run.apply(ActivityEvent::StageCleared(SEASON));
        run.apply(ActivityEvent::ModeEntered(SEASON));
        assert_eq!(run.modes[&SEASON].attempts, 1);
        assert_eq!(run.modes[&SEASON].cleared, 1);
        assert_eq!(run.modes[&GameMode::AfkStages].attempts, 0);
    }

    #[test]
    fn ratios_exclude_pending_unclear_and_unfinished_stages() {
        let mut run = RunActivity::default();
        run.start(SEASON, Instant::now());
        assert_eq!(run.success_rate(), None);
        assert_eq!(run.attempts_per_clear(), None);
        for outcome in [BattleOutcome::Lost, BattleOutcome::Won] {
            run.apply(ActivityEvent::BattleStarted(SEASON));
            run.apply(ActivityEvent::BattleFinished(SEASON, outcome));
        }
        run.apply(ActivityEvent::StageCleared(SEASON));
        run.apply(ActivityEvent::StageCleared(SEASON));
        run.apply(ActivityEvent::BattleFinished(SEASON, BattleOutcome::Won));
        run.apply(ActivityEvent::BattleStarted(SEASON));
        run.apply(ActivityEvent::BattleFinished(
            SEASON,
            BattleOutcome::Unclear,
        ));
        run.apply(ActivityEvent::BattleStarted(SEASON));
        assert_eq!(run.success_rate(), Some(50.0));
        assert_eq!(run.attempts_per_clear(), Some(2.0));
        assert_eq!(run.modes[&SEASON].cleared, 1);
    }

    #[test]
    fn stopping_freezes_duration_and_a_new_run_resets_stats() {
        let now = Instant::now();
        let mut run = RunActivity::default();
        run.start(SEASON, now);
        run.apply(ActivityEvent::BattleStarted(SEASON));
        run.finish(now + Duration::from_secs(12));
        run.apply(ActivityEvent::StageCleared(SEASON));
        assert_eq!(
            run.elapsed(now + Duration::from_secs(100)),
            Duration::from_secs(12)
        );
        run.start(SEASON, now + Duration::from_secs(100));
        assert!(run.modes.is_empty());
        assert_eq!(run.current_mode, None);
        assert_eq!(
            run.elapsed(now + Duration::from_secs(101)),
            Duration::from_secs(1)
        );
    }

    #[test]
    fn chart_history_records_only_confirmed_clears_across_modes_and_resets() {
        let now = Instant::now();
        let mut run = RunActivity::default();
        run.start(SEASON, now);
        assert_eq!(run.stage_history.len(), 1);
        assert_eq!(run.stage_history[0].cleared, 0);
        run.apply_at(
            ActivityEvent::BattleStarted(SEASON),
            now + Duration::from_secs(1),
        );
        run.apply_at(
            ActivityEvent::BattleFinished(SEASON, BattleOutcome::Lost),
            now + Duration::from_secs(2),
        );
        assert_eq!(run.stage_history.len(), 1);
        for (mode, time) in [(SEASON, 4), (GameMode::AfkStages, 10)] {
            run.apply_at(
                ActivityEvent::ModeEntered(mode),
                now + Duration::from_secs(time),
            );
            run.apply_at(
                ActivityEvent::BattleStarted(mode),
                now + Duration::from_secs(time),
            );
            run.apply_at(
                ActivityEvent::BattleFinished(mode, BattleOutcome::Won),
                now + Duration::from_secs(time),
            );
            run.apply_at(
                ActivityEvent::StageCleared(mode),
                now + Duration::from_secs(time),
            );
            run.apply_at(
                ActivityEvent::StageCleared(mode),
                now + Duration::from_secs(time),
            );
        }
        assert_eq!(run.stage_history.len(), 3);
        assert_eq!(run.stage_history[1].elapsed, Duration::from_secs(4));
        assert_eq!(run.stage_history[1].cleared, 1);
        assert_eq!(run.stage_history[2].elapsed, Duration::from_secs(10));
        assert_eq!(run.stage_history[2].cleared, 2);
        run.finish(now + Duration::from_secs(12));
        run.apply_at(
            ActivityEvent::StageCleared(SEASON),
            now + Duration::from_secs(15),
        );
        assert_eq!(run.stage_history.len(), 3);
        run.start(SEASON, now + Duration::from_secs(20));
        assert_eq!(run.stage_history.len(), 1);
        assert_eq!(run.stage_history[0].cleared, 0);
    }
}
