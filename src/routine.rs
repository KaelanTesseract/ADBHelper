use crate::{activity::GameMode, localization::text};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    AfkStages,
    SeasonLegendTrial,
    UnionCampaign,
    DurasTrials,
    Arena,
    SupremeArena,
    Daily,
    AfkRewards,
    FreeHourglasses,
    DiamondHourglasses,
    StageRewards,
    SeasonRankingRewards,
}

impl Action {
    pub const DAILY_STEPS: [Self; 11] = [
        Self::AfkRewards,
        Self::FreeHourglasses,
        Self::DiamondHourglasses,
        Self::StageRewards,
        Self::SeasonRankingRewards,
        Self::AfkStages,
        Self::SeasonLegendTrial,
        Self::UnionCampaign,
        Self::DurasTrials,
        Self::Arena,
        Self::SupremeArena,
    ];

    pub fn index(self) -> usize {
        match self {
            Self::AfkStages => 0,
            Self::SeasonLegendTrial => 1,
            Self::UnionCampaign => 2,
            Self::DurasTrials => 3,
            Self::Arena => 4,
            Self::SupremeArena => 5,
            Self::Daily => 6,
            Self::AfkRewards => 7,
            Self::FreeHourglasses => 8,
            Self::DiamondHourglasses => 9,
            Self::StageRewards => 10,
            Self::SeasonRankingRewards => 11,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::AfkStages => GameMode::AfkStages.label(),
            Self::SeasonLegendTrial => "Season Legend Trial",
            Self::UnionCampaign => "Union Campaign",
            Self::DurasTrials => "Dura’s Trials",
            Self::Arena => "Arena",
            Self::SupremeArena => "Supreme Arena",
            Self::Daily => "Daily",
            Self::AfkRewards => text("AFK rewards · 2 claims", "AFK-Belohnungen · 2× abholen"),
            Self::FreeHourglasses => text(
                "Free hourglasses · up to 3",
                "Kostenlose Sanduhren · bis zu 3",
            ),
            Self::DiamondHourglasses => text("Diamond hourglasses", "Diamanten-Sanduhren"),
            Self::StageRewards => text("AFK stage rewards", "AFK-Stufenbelohnungen"),
            Self::SeasonRankingRewards => text(
                "Season Phase Ranking rewards",
                "Season-Phase-Ranglistenbelohnungen",
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Routine {
    pub steps: Vec<Action>,
    #[serde(default)]
    pub diamond_hourglasses: DiamondHourglassSettings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiamondHourglassSettings {
    pub purchases: u32,
    pub budget: u32,
}

impl Default for DiamondHourglassSettings {
    fn default() -> Self {
        Self {
            purchases: 1,
            budget: 50,
        }
    }
}

impl DiamondHourglassSettings {
    pub fn permits(self, count: u32, reserved: u32, price: u32) -> bool {
        price > 0
            && count < self.purchases.min(10)
            && reserved
                .checked_add(price)
                .is_some_and(|cost| cost <= self.budget.min(5000))
    }
}

impl Default for Routine {
    fn default() -> Self {
        Self {
            steps: vec![Action::AfkRewards, Action::FreeHourglasses],
            diamond_hourglasses: DiamondHourglassSettings::default(),
        }
    }
}

impl Routine {
    pub fn run(
        &self,
        cancelled: &std::sync::atomic::AtomicBool,
        mut execute: impl FnMut(usize, Action) -> Result<u32, String>,
    ) -> Result<u32, String> {
        self.validate()?;
        let mut total = 0u32;
        for (index, step) in self.steps.iter().copied().enumerate() {
            crate::afk_journey::afk_stages::ensure_not_cancelled(cancelled)?;
            total = total.saturating_add(execute(index, step)?);
        }
        Ok(total)
    }
    pub fn validate(&self) -> Result<(), String> {
        for (index, action) in self.steps.iter().enumerate() {
            if *action == Action::Daily || self.steps[..index].contains(action) {
                return Err(text(
                    "The Daily routine contains an invalid or duplicate step.",
                    "Die Daily-Routine enthält einen ungültigen oder doppelten Schritt.",
                )
                .into());
            }
        }
        Ok(())
    }

    pub fn place(&mut self, action: Action, mut position: usize) {
        if action == Action::Daily {
            return;
        }
        position = position.min(self.steps.len());
        if let Some(previous) = self.steps.iter().position(|step| *step == action) {
            self.steps.remove(previous);
            if previous < position {
                position -= 1;
            }
        }
        self.steps.insert(position, action);
    }

    pub fn load() -> Result<Self, String> {
        let path = crate::settings::file("daily-routine.json").map_err(|e| e.to_string())?;
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default())
            }
            Err(error) => return Err(error.to_string()),
        };
        let routine: Self = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        routine.validate()?;
        Ok(routine)
    }

    pub fn save(&self) -> Result<(), String> {
        self.validate()?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        crate::settings::write("daily-routine.json", &bytes).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paid_budget_is_bounded_and_old_routines_remain_free_only() {
        let old: Routine =
            serde_json::from_str(r#"{"steps":["afk_rewards","free_hourglasses"]}"#).unwrap();
        assert_eq!(old, Routine::default());
        let settings = DiamondHourglassSettings {
            purchases: 3,
            budget: 150,
        };
        assert!(settings.permits(0, 0, 50));
        assert!(settings.permits(1, 50, 100));
        assert!(!settings.permits(2, 150, 100));
        assert!(!settings.permits(3, 0, 50));
        assert!(!settings.permits(0, 0, 0));
        assert!(!settings.permits(0, u32::MAX, 50));
        assert!(!DiamondHourglassSettings {
            purchases: 1,
            budget: 0
        }
        .permits(0, 0, 50));
    }

    #[test]
    fn moving_steps_preserves_order_and_rejects_nested_or_duplicate_routines() {
        let mut routine = Routine::default();
        routine.place(Action::Arena, 1);
        routine.place(Action::AfkRewards, 3);
        assert_eq!(
            routine.steps,
            [Action::Arena, Action::FreeHourglasses, Action::AfkRewards]
        );
        routine.place(Action::FreeHourglasses, 0);
        routine.place(Action::Daily, 0);
        assert_eq!(
            routine.steps,
            [Action::FreeHourglasses, Action::Arena, Action::AfkRewards]
        );
        let restored: Routine =
            serde_json::from_slice(&serde_json::to_vec(&routine).unwrap()).unwrap();
        assert_eq!(restored, routine);
        routine.steps.push(Action::Arena);
        assert!(routine.validate().is_err());
        routine.steps = vec![Action::Daily];
        assert!(routine.validate().is_err());
    }

    #[test]
    fn stop_or_a_step_error_prevents_later_actions() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let cancelled = AtomicBool::new(false);
        let mut seen = Vec::new();
        assert!(Routine::default()
            .run(&cancelled, |_, step| {
                seen.push(step);
                cancelled.store(true, Ordering::Relaxed);
                Ok(1)
            })
            .is_err());
        assert_eq!(seen, [Action::AfkRewards]);
        seen.clear();
        assert!(Routine::default()
            .run(&AtomicBool::new(false), |_, step| {
                seen.push(step);
                Err("device offline".into())
            })
            .is_err());
        assert_eq!(seen, [Action::AfkRewards]);
    }
}
