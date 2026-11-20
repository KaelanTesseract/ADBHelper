//! Template images embedded in the executable, decoded once and cached.

use crate::localization::{format_text, text};
use image::GrayImage;
use std::sync::{Mutex, OnceLock};

fn embedded_template(relative_path: &str) -> Result<&'static [u8], String> {
    let bytes: &'static [u8] = match relative_path {
        "dream_realm/label.png" => {
            include_bytes!("../../../assets/AFKJourney/dream_realm/label.png")
        }
        "dream_realm/dr_ranking.png" => {
            include_bytes!("../../../assets/AFKJourney/dream_realm/dr_ranking.png")
        }
        "dream_realm/daily_done.png" => {
            include_bytes!("../../../assets/AFKJourney/dream_realm/daily_done.png")
        }
        "dream_realm/done.png" => include_bytes!("../../../assets/AFKJourney/dream_realm/done.png"),
        "dream_realm/buy.png" => include_bytes!("../../../assets/AFKJourney/dream_realm/buy.png"),
        "dream_realm/gold_price.png" => {
            include_bytes!("../../../assets/AFKJourney/dream_realm/gold_price.png")
        }
        "dream_realm/battle_ended.png" => {
            include_bytes!("../../../assets/AFKJourney/dream_realm/battle_ended.png")
        }
        "dream_realm/skip_formation.png" => {
            include_bytes!("../../../assets/AFKJourney/dream_realm/skip_formation.png")
        }
        "daily/afk_time.png" => include_bytes!("../../../assets/AFKJourney/daily/afk_time.png"),
        "afk_stages/ranking_button.png" => {
            include_bytes!("../../../assets/AFKJourney/afk_stages/ranking_button.png")
        }
        "daily/wallet_diamond.png" => {
            include_bytes!("../../../assets/AFKJourney/daily/wallet_diamond.png")
        }
        "daily/timer_reset.png" => {
            include_bytes!("../../../assets/AFKJourney/daily/timer_reset.png")
        }
        "daily/claim.png" => include_bytes!("../../../assets/AFKJourney/daily/claim.png"),
        "back.png" => include_bytes!("../../../assets/AFKJourney/back.png"),
        "progression/hero_model_eye.png" => {
            include_bytes!("../../../assets/AFKJourney/progression/hero_model_eye.png")
        }
        "progression/hero_profile_tab.png" => {
            include_bytes!("../../../assets/AFKJourney/progression/hero_profile_tab.png")
        }
        "progression/scan_paragon_three.png" => {
            include_bytes!("../../../assets/AFKJourney/progression/scan_paragon_three.png")
        }
        "progression/scan_crown.png" => {
            include_bytes!("../../../assets/AFKJourney/progression/scan_crown.png")
        }
        "progression/scan_paragon_badge.png" => {
            include_bytes!("../../../assets/AFKJourney/progression/scan_paragon_badge.png")
        }
        "progression/hero_affinity_icon.png" => {
            include_bytes!("../../../assets/AFKJourney/progression/hero_affinity_icon.png")
        }
        "resonating_hall/level_up_all.png" => {
            include_bytes!("../../../assets/AFKJourney/resonating_hall/level_up_all.png")
        }
        "resonating_hall/equipment.png" => {
            include_bytes!("../../../assets/AFKJourney/resonating_hall/equipment.png")
        }
        "resonating_hall/action.png" => {
            include_bytes!("../../../assets/AFKJourney/resonating_hall/action.png")
        }
        "resonating_hall/hero_action.png" => {
            include_bytes!("../../../assets/AFKJourney/resonating_hall/hero_action.png")
        }
        "dailies/resonating_hall/chippy.png" => {
            include_bytes!("../../../assets/AFKJourney/dailies/resonating_hall/chippy.png")
        }
        "equipment/quick_equip.png" => {
            include_bytes!("../../../assets/AFKJourney/equipment/quick_equip.png")
        }
        "equipment/open_all.png" => {
            include_bytes!("../../../assets/AFKJourney/equipment/open_all.png")
        }
        "equipment/support.png" => {
            include_bytes!("../../../assets/AFKJourney/equipment/support.png")
        }
        "equipment/mage.png" => include_bytes!("../../../assets/AFKJourney/equipment/mage.png"),
        "equipment/warrior.png" => {
            include_bytes!("../../../assets/AFKJourney/equipment/warrior.png")
        }
        "equipment/rogue.png" => include_bytes!("../../../assets/AFKJourney/equipment/rogue.png"),
        "equipment/marksman.png" => {
            include_bytes!("../../../assets/AFKJourney/equipment/marksman.png")
        }
        "equipment/tank.png" => include_bytes!("../../../assets/AFKJourney/equipment/tank.png"),
        "dailies/emporium/invite_letter.png" => {
            include_bytes!("../../../assets/AFKJourney/dailies/emporium/invite_letter.png")
        }
        "dailies/emporium/discount_affinity.png" => {
            include_bytes!("../../../assets/AFKJourney/dailies/emporium/discount_affinity.png")
        }
        "dailies/emporium/other_affinity.png" => {
            include_bytes!("../../../assets/AFKJourney/dailies/emporium/other_affinity.png")
        }
        "dailies/emporium/bound_essence.png" => {
            include_bytes!("../../../assets/AFKJourney/dailies/emporium/bound_essence.png")
        }
        "dailies/noble_tavern/all_hero_recruit.png" => {
            include_bytes!("../../../assets/AFKJourney/dailies/noble_tavern/all_hero_recruit.png")
        }
        "dailies/noble_tavern/maxed_hero_continue.png" => {
            include_bytes!(
                "../../../assets/AFKJourney/dailies/noble_tavern/maxed_hero_continue.png"
            )
        }
        "progression/diamond.png" => {
            include_bytes!("../../../assets/AFKJourney/progression/diamond.png")
        }
        "progression/pal_coin.png" => {
            include_bytes!("../../../assets/AFKJourney/progression/pal_coin.png")
        }
        "progression/dream_fragment.png" => {
            include_bytes!("../../../assets/AFKJourney/progression/dream_fragment.png")
        }
        "progression/invite_cost.png" => {
            include_bytes!("../../../assets/AFKJourney/progression/invite_cost.png")
        }
        "dailies/hamburger/rewards_available.png" => {
            include_bytes!("../../../assets/AFKJourney/dailies/hamburger/rewards_available.png")
        }
        "daily/free_hourglass.png" => {
            include_bytes!("../../../assets/AFKJourney/daily/free_hourglass.png")
        }
        "daily/hourglass_zero.png" => {
            include_bytes!("../../../assets/AFKJourney/daily/hourglass_zero.png")
        }
        "daily/paid_hourglass.png" => {
            include_bytes!("../../../assets/AFKJourney/daily/paid_hourglass.png")
        }
        "arena/free_price_zero.png" => {
            include_bytes!("../../../assets/AFKJourney/arena/free_price_zero.png")
        }
        "arena/battle.png" => include_bytes!("../../../assets/AFKJourney/arena/battle.png"),
        "arena/buy.png" => include_bytes!("../../../assets/AFKJourney/arena/buy.png"),
        "arena/buy_free.png" => include_bytes!("../../../assets/AFKJourney/arena/buy_free.png"),
        "arena/cancel_purchase.png" => {
            include_bytes!("../../../assets/AFKJourney/arena/cancel_purchase.png")
        }
        "arena/challenge.png" => include_bytes!("../../../assets/AFKJourney/arena/challenge.png"),
        "arena/done.png" => include_bytes!("../../../assets/AFKJourney/arena/done.png"),
        "arena/label.png" => include_bytes!("../../../assets/AFKJourney/arena/label.png"),
        "arena/home_label.png" => include_bytes!("../../../assets/AFKJourney/arena/home_label.png"),
        "arena/opponent.png" => include_bytes!("../../../assets/AFKJourney/arena/opponent.png"),
        "arena/skip.png" => include_bytes!("../../../assets/AFKJourney/arena/skip.png"),
        "arena/weekly_notice.png" => {
            include_bytes!("../../../assets/AFKJourney/arena/weekly_notice.png")
        }
        "arena/weekly_rewards.png" => {
            include_bytes!("../../../assets/AFKJourney/arena/weekly_rewards.png")
        }
        "supreme_arena/challenge.png" => {
            include_bytes!("../../../assets/AFKJourney/supreme_arena/challenge.png")
        }
        "supreme_arena/challenge_detail.png" => {
            include_bytes!("../../../assets/AFKJourney/supreme_arena/challenge_detail.png")
        }
        "supreme_arena/daily_rewards.png" => {
            include_bytes!("../../../assets/AFKJourney/supreme_arena/daily_rewards.png")
        }
        "supreme_arena/label.png" => {
            include_bytes!("../../../assets/AFKJourney/supreme_arena/label.png")
        }
        "supreme_arena/no_attempts_popup.png" => {
            include_bytes!("../../../assets/AFKJourney/supreme_arena/no_attempts_popup.png")
        }
        "supreme_arena/rankings.png" => {
            include_bytes!("../../../assets/AFKJourney/supreme_arena/rankings.png")
        }
        "supreme_arena/select_opponent.png" => {
            include_bytes!("../../../assets/AFKJourney/supreme_arena/select_opponent.png")
        }
        "battle_modes/supreme_arena.png" => {
            include_bytes!("../../../assets/AFKJourney/battle_modes/supreme_arena.png")
        }
        "battle_modes/duras_trials.png" => {
            include_bytes!("../../../assets/AFKJourney/battle_modes/duras_trials.png")
        }
        "duras_trials/battle.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/battle.png")
        }
        "duras_trials/cleared.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/cleared.png")
        }
        "duras_trials/continue_gray.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/continue_gray.png")
        }
        "duras_trials/end_sunrise.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/end_sunrise.png")
        }
        "duras_trials/first_clear_bottom_half.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/first_clear_bottom_half.png")
        }
        "duras_trials/no_next.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/no_next.png")
        }
        "duras_trials/sweep_quantity_one.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/sweep_quantity_one.png")
        }
        "duras_trials/sweep_plus.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/sweep_plus.png")
        }
        "duras_trials/sweep_daily_token.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/sweep_daily_token.png")
        }
        "duras_trials/socketed_charms_overview.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/socketed_charms_overview.png")
        }
        "duras_trials/sweep.png" => {
            include_bytes!("../../../assets/AFKJourney/duras_trials/sweep.png")
        }
        "battle_modes/union_campaign.png" => {
            include_bytes!("../../../assets/AFKJourney/battle_modes/union_campaign.png")
        }
        "battle_modes/guild_mode_tab.png" => {
            include_bytes!("../../../assets/AFKJourney/battle_modes/guild_mode_tab.png")
        }
        "union_campaign/current_label.png" => {
            include_bytes!("../../../assets/AFKJourney/union_campaign/current_label.png")
        }
        "union_campaign/battle.png" => {
            include_bytes!("../../../assets/AFKJourney/union_campaign/battle.png")
        }
        "arena/continue.png" => include_bytes!("../../../assets/AFKJourney/arena/continue.png"),
        "quests/skip.png" => include_bytes!("../../../assets/AFKJourney/quests/skip.png"),
        "battle_modes/legend_trial.png" => {
            include_bytes!("../../../assets/AFKJourney/battle_modes/legend_trial.png")
        }
        "battle_modes/coming_soon.png" => {
            include_bytes!("../../../assets/AFKJourney/battle_modes/coming_soon.png")
        }
        "tap_to_close.png" => include_bytes!("../../../assets/AFKJourney/tap_to_close.png"),
        "legend_trials/available_after.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/available_after.png")
        }
        "legend_trials/banner_graveborn.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/banner_graveborn.png")
        }
        "legend_trials/banner_lightbearer.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/banner_lightbearer.png")
        }
        "legend_trials/banner_mauler.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/banner_mauler.png")
        }
        "legend_trials/banner_wilder.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/banner_wilder.png")
        }
        "legend_trials/challenge_ch.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/challenge_ch.png")
        }
        "legend_trials/challenge_ge.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/challenge_ge.png")
        }
        "legend_trials/faction_icon_graveborn.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/faction_icon_graveborn.png")
        }
        "legend_trials/faction_icon_lightbearer.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/faction_icon_lightbearer.png")
        }
        "legend_trials/faction_icon_mauler.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/faction_icon_mauler.png")
        }
        "legend_trials/faction_icon_wilder.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/faction_icon_wilder.png")
        }
        "legend_trials/s_header.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/s_header.png")
        }
        "legend_trials/top_floor_reached.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/top_floor_reached.png")
        }
        "legend_trials/tower_icon_graveborn.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/tower_icon_graveborn.png")
        }
        "legend_trials/tower_icon_lightbearer.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/tower_icon_lightbearer.png")
        }
        "legend_trials/tower_icon_mauler.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/tower_icon_mauler.png")
        }
        "legend_trials/tower_icon_wilder.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/tower_icon_wilder.png")
        }
        "legend_trials/unlocks_at_season_phase.png" => {
            include_bytes!("../../../assets/AFKJourney/legend_trials/unlocks_at_season_phase.png")
        }
        "battle_modes/afk_stage.png" => {
            include_bytes!("../../../assets/AFKJourney/battle_modes/afk_stage.png")
        }
        "navigation/time_of_day.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/time_of_day.png")
        }
        "navigation/resonating_hall_shortcut.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/resonating_hall_shortcut.png")
        }
        "navigation/homestead/homestead_enter.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/homestead/homestead_enter.png")
        }
        "navigation/homestead/homestead_invaded.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/homestead/homestead_invaded.png")
        }
        "navigation/homestead/world.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/homestead/world.png")
        }
        "guide/next.png" => include_bytes!("../../../assets/AFKJourney/guide/next.png"),
        "popup/quick_purchase.png" => {
            include_bytes!("../../../assets/AFKJourney/popup/quick_purchase.png")
        }
        "navigation/dotdotdot.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/dotdotdot.png")
        }
        "navigation/confirm_text.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/confirm_text.png")
        }
        "navigation/resonating_hall_label.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/resonating_hall_label.png")
        }
        "navigation/hamburger_menu.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/hamburger_menu.png")
        }
        "navigation/apex_stage_shortcut.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/apex_stage_shortcut.png")
        }
        "arcane_labyrinth/select_a_crest.png" => {
            include_bytes!("../../../assets/AFKJourney/arcane_labyrinth/select_a_crest.png")
        }
        "navigation/notice.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/notice.png")
        }
        "navigation/x.png" => include_bytes!("../../../assets/AFKJourney/navigation/x.png"),
        "navigation/resonating_hall_back.png" => {
            include_bytes!("../../../assets/AFKJourney/navigation/resonating_hall_back.png")
        }
        "guide/close.png" => include_bytes!("../../../assets/AFKJourney/guide/close.png"),
        "login/claim.png" => include_bytes!("../../../assets/AFKJourney/login/claim.png"),
        "arcane_labyrinth/back_arrow.png" => {
            include_bytes!("../../../assets/AFKJourney/arcane_labyrinth/back_arrow.png")
        }
        "battle/exit_door.png" => include_bytes!("../../../assets/AFKJourney/battle/exit_door.png"),
        "battle/skip.png" => include_bytes!("../../../assets/AFKJourney/battle/skip.png"),
        "battle/skip_orange.png" => {
            include_bytes!("../../../assets/AFKJourney/battle/skip_orange.png")
        }
        "afk_stages/battle_large.png" => {
            &include_bytes!("../../../assets/AFKJourney/afk_stages/battle_large.png")[..]
        }
        "afk_stages/talent_trials_large.png" => {
            include_bytes!("../../../assets/AFKJourney/afk_stages/talent_trials_large.png")
        }
        "afk_stages/season_battle.png" => {
            &include_bytes!("../../../assets/AFKJourney/afk_stages/season_battle.png")[..]
        }
        "afk_stages/tap_to_close.png" => {
            &include_bytes!("../../../assets/AFKJourney/afk_stages/tap_to_close.png")[..]
        }
        "battle/battle.png" => &include_bytes!("../../../assets/AFKJourney/battle/battle.png")[..],
        "battle/next.png" => include_bytes!("../../../assets/AFKJourney/battle/next.png"),
        "battle/victory_rewards_wide.png" => {
            include_bytes!("../../../assets/AFKJourney/event/sunlit_showdown/victory.png")
        }
        "battle/copy.png" => &include_bytes!("../../../assets/AFKJourney/battle/copy.png")[..],
        "battle/formation_next.png" => {
            &include_bytes!("../../../assets/AFKJourney/battle/formation_next.png")[..]
        }
        "battle/formations_icon.png" => {
            &include_bytes!("../../../assets/AFKJourney/battle/formations_icon.png")[..]
        }
        "battle/gold.png" => &include_bytes!("../../../assets/AFKJourney/battle/gold.png")[..],
        "battle/manual_battle.png" => {
            &include_bytes!("../../../assets/AFKJourney/battle/manual_battle.png")[..]
        }
        "battle/manual_battle_old1.png" => {
            &include_bytes!("../../../assets/AFKJourney/battle/manual_battle_old1.png")[..]
        }
        "battle/power_up.png" => {
            &include_bytes!("../../../assets/AFKJourney/battle/power_up.png")[..]
        }
        "battle/records.png" => {
            &include_bytes!("../../../assets/AFKJourney/battle/records.png")[..]
        }
        "battle/result.png" => &include_bytes!("../../../assets/AFKJourney/battle/result.png")[..],
        "battle/spend.png" => &include_bytes!("../../../assets/AFKJourney/battle/spend.png")[..],
        "battle/victory_rewards.png" => {
            &include_bytes!("../../../assets/AFKJourney/battle/victory_rewards.png")[..]
        }
        "cancel.png" => &include_bytes!("../../../assets/AFKJourney/cancel.png")[..],
        "confirm_text.png" => &include_bytes!("../../../assets/AFKJourney/confirm_text.png")[..],
        "navigation/confirm.png" => {
            &include_bytes!("../../../assets/AFKJourney/navigation/confirm.png")[..]
        }
        "next.png" => &include_bytes!("../../../assets/AFKJourney/next.png")[..],
        "retry.png" => &include_bytes!("../../../assets/AFKJourney/retry.png")[..],
        "fishing/book.png" => include_bytes!("../../../assets/AFKJourney/fishing/book.png"),
        "fishing/fishing_rod.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/fishing_rod.png")
        }
        "fishing/fishing_rod_big.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/fishing_rod_big.png")
        }
        "fishing/hook_fish.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/hook_fish.png")
        }
        "fishing/hook_fish_big.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/hook_fish_big.png")
        }
        "fishing/hook.png" => include_bytes!("../../../assets/AFKJourney/fishing/hook.png"),
        "fishing/hook_held.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/hook_held.png")
        }
        "fishing/fishing_level.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/fishing_level.png")
        }
        "fishing/fishing_level_big.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/fishing_level_big.png")
        }
        "fishing/fishing_level_info_button.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/fishing_level_info_button.png")
        }
        "fishing/silver_medal.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/silver_medal.png")
        }
        "fishing/silver_medal2.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/silver_medal2.png")
        }
        "fishing/gold_medal.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/gold_medal.png")
        }
        "fishing/gold_medal2.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/gold_medal2.png")
        }
        "fishing/map_marker.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/map_marker.png")
        }
        "fishing/map_walk.png" => include_bytes!("../../../assets/AFKJourney/fishing/map_walk.png"),
        "fishing/map_counter.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/map_counter.png")
        }
        "fishing/map_marker_small.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/map_marker_small.png")
        }
        "fishing/map_marker_edge_small.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/map_marker_edge_small.png")
        }
        "fishing/map_marker_edge.png" => {
            include_bytes!("../../../assets/AFKJourney/fishing/map_marker_edge.png")
        }
        _ => {
            return Err(format_text!(
                "Template is not embedded in this build: {relative_path}",
                "Die Vorlage fehlt in dieser Version: {relative_path}"
            ))
        }
    };
    Ok(bytes)
}

pub(in crate::afk_journey) fn load_template(relative_path: &str) -> Result<GrayImage, String> {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, GrayImage>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Some(image) = cache
        .lock()
        .map_err(|_| {
            text(
                "template cache is unavailable",
                "Der Vorlagenspeicher ist nicht verfügbar",
            )
            .to_owned()
        })?
        .get(relative_path)
        .cloned()
    {
        return Ok(image);
    }

    let image = image::load_from_memory(embedded_template(relative_path)?)
        .map_err(|error| {
            format_text!(
                "Could not decode template {relative_path}: {error}",
                "Die Vorlage {relative_path} konnte nicht gelesen werden: {error}"
            )
        })?
        .to_luma8();
    cache
        .lock()
        .map_err(|_| {
            text(
                "template cache is unavailable",
                "Der Vorlagenspeicher ist nicht verfügbar",
            )
            .to_owned()
        })?
        .insert(relative_path.to_owned(), image.clone());
    Ok(image)
}
