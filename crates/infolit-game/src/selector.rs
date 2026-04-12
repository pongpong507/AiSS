//! # Selector 模組
//!
//! Affinity 加權挑選演員與騙術組合。

use crate::actor::Actor;
use crate::deception::DeceptionPattern;
use rand::prelude::*;
use std::collections::HashMap;

/// 一局組裝結果：選定的演員、騙子 id 列表、每個騙子對應的騙術
pub type AssembledSession = (Vec<Actor>, Vec<String>, HashMap<String, DeceptionPattern>);

/// 從演員池中組裝一局遊戲的陣容
///
/// - 從 `actors` 中隨機抽 `actor_count` 個演員
/// - 指派其中 `liar_count` 個為騙子
/// - 為每個騙子依 affinity 加權挑選騙術（15% 完全隨機例外）
pub fn assemble_session(
    actors: &[Actor],
    catalog: &[DeceptionPattern],
    actor_count: usize,
    liar_count: usize,
) -> anyhow::Result<AssembledSession> {
    if actors.len() < actor_count {
        anyhow::bail!("演員池不足：需要 {}，實際只有 {}", actor_count, actors.len());
    }
    if liar_count > actor_count {
        anyhow::bail!("騙子數量不能超過演員數量");
    }
    if catalog.is_empty() {
        anyhow::bail!("騙術清單為空");
    }

    let mut rng = thread_rng();

    // 隨機抽演員
    let mut selected: Vec<Actor> = actors.to_vec();
    selected.shuffle(&mut rng);
    let selected = selected[..actor_count].to_vec();

    // 隨機指定騙子（從 selected 中隨機挑，而非固定取前 N 個）
    let mut liar_candidates: Vec<usize> = (0..actor_count).collect();
    liar_candidates.shuffle(&mut rng);
    let liar_ids: Vec<String> = liar_candidates[..liar_count]
        .iter()
        .map(|&i| selected[i].id.clone())
        .collect();

    // 為每個騙子指派騙術
    let mut deceptions = HashMap::new();
    for liar_id in &liar_ids {
        let actor = selected.iter().find(|a| &a.id == liar_id).unwrap();
        let pattern = pick_deception(&mut rng, actor, catalog);
        deceptions.insert(liar_id.clone(), pattern);
    }

    Ok((selected, liar_ids, deceptions))
}

/// Affinity 加權挑選騙術
///
/// - 85%：依 affinity 距離加權（距離越近權重越高）
/// - 15%：完全隨機（確保多樣性、防 metagame）
fn pick_deception(rng: &mut impl Rng, actor: &Actor, catalog: &[DeceptionPattern]) -> DeceptionPattern {
    // 15% 完全隨機例外通道
    if rng.gen_bool(0.15) {
        return catalog.choose(rng).unwrap().clone();
    }

    // 85%：依 affinity 距離加權
    let weights: Vec<f64> = catalog
        .iter()
        .map(|p| {
            let distance = (actor.affinity as i32 - p.affinity as i32).unsigned_abs() as f64;
            1.0 / (1.0 + distance)
        })
        .collect();

    let dist = rand::distributions::WeightedIndex::new(&weights)
        .expect("weights 不能全為 0");
    catalog[dist.sample(rng)].clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor::Actor;
    use crate::deception::DeceptionPattern;
    use shared_types::Difficulty;

    fn make_actor(id: &str, affinity: u8) -> Actor {
        Actor {
            id: id.to_string(),
            name: id.to_string(),
            avatar: String::new(),
            short_bio: String::new(),
            personality_traits: vec![],
            speech_style: String::new(),
            affinity,
            eagerness: 5,
        }
    }

    fn make_deception(id: &str, affinity: u8) -> DeceptionPattern {
        DeceptionPattern {
            id: id.to_string(),
            name_zh: id.to_string(),
            description: String::new(),
            example: String::new(),
            difficulty: Difficulty::Easy,
            teaching_goal: String::new(),
            affinity,
        }
    }

    #[test]
    fn test_assemble_basic() {
        let actors: Vec<Actor> = (0..5).map(|i| make_actor(&format!("a{i}"), (i as u8 * 2) + 1)).collect();
        let catalog: Vec<DeceptionPattern> = (0..3).map(|i| make_deception(&format!("d{i}"), (i as u8 * 3) + 3)).collect();

        let (selected, liars, deceptions) = assemble_session(&actors, &catalog, 3, 1).unwrap();
        assert_eq!(selected.len(), 3);
        assert_eq!(liars.len(), 1);
        assert_eq!(deceptions.len(), 1);
        assert!(liars.iter().all(|id| deceptions.contains_key(id)));
    }

    #[test]
    fn assemble_fails_when_pool_too_small() {
        let actors = vec![make_actor("a1", 5)];
        let catalog = vec![make_deception("d1", 5)];
        let result = assemble_session(&actors, &catalog, 3, 1);
        assert!(result.is_err());
    }

    #[test]
    fn assemble_fails_when_liars_exceed_actors() {
        let actors: Vec<Actor> = (0..3).map(|i| make_actor(&format!("a{i}"), 5)).collect();
        let catalog = vec![make_deception("d1", 5)];
        let result = assemble_session(&actors, &catalog, 3, 5);
        assert!(result.is_err());
    }

    #[test]
    fn assemble_fails_when_catalog_empty() {
        let actors: Vec<Actor> = (0..3).map(|i| make_actor(&format!("a{i}"), 5)).collect();
        let catalog: Vec<DeceptionPattern> = vec![];
        let result = assemble_session(&actors, &catalog, 3, 1);
        assert!(result.is_err());
    }

    #[test]
    fn assemble_zero_liars_works() {
        let actors: Vec<Actor> = (0..3).map(|i| make_actor(&format!("a{i}"), 5)).collect();
        let catalog = vec![make_deception("d1", 5)];
        let (selected, liars, deceptions) =
            assemble_session(&actors, &catalog, 3, 0).unwrap();
        assert_eq!(selected.len(), 3);
        assert_eq!(liars.len(), 0);
        assert_eq!(deceptions.len(), 0);
    }

    #[test]
    fn assemble_all_liars_works() {
        let actors: Vec<Actor> = (0..3).map(|i| make_actor(&format!("a{i}"), 5)).collect();
        let catalog = vec![make_deception("d1", 5), make_deception("d2", 6)];
        let (selected, liars, deceptions) =
            assemble_session(&actors, &catalog, 3, 3).unwrap();
        assert_eq!(selected.len(), 3);
        assert_eq!(liars.len(), 3);
        assert_eq!(deceptions.len(), 3);
        // 每個騙子都有對應的騙術
        for liar_id in &liars {
            assert!(deceptions.contains_key(liar_id));
        }
    }

    #[test]
    fn affinity_distance_zero_gives_max_weight() {
        // 演員 affinity=6，所有騙術 affinity 都不同
        let actor = make_actor("a", 6);
        let catalog = vec![
            make_deception("d_dist_0", 6),
            make_deception("d_dist_3", 9),
            make_deception("d_dist_5", 11),
        ];

        let mut counts = std::collections::HashMap::new();
        let mut rng = thread_rng();
        for _ in 0..1000 {
            let picked = pick_deception(&mut rng, &actor, &catalog);
            *counts.entry(picked.id.clone()).or_insert(0u32) += 1;
        }
        let zero = *counts.get("d_dist_0").unwrap_or(&0);
        let three = *counts.get("d_dist_3").unwrap_or(&0);
        let five = *counts.get("d_dist_5").unwrap_or(&0);
        // 距離 0 的應該被選最多次
        assert!(zero > three, "distance=0 ({}) should beat distance=3 ({})", zero, three);
        assert!(three > five, "distance=3 ({}) should beat distance=5 ({})", three, five);
    }

    #[test]
    fn test_affinity_weighting_high_affinity_match() {
        // 演員 affinity=9，騙術清單中 affinity=9 的應被高頻選中
        let actor = make_actor("a_high", 9);
        let catalog = vec![
            make_deception("d_match", 9),  // 距離 0，最高權重
            make_deception("d_far", 1),    // 距離 8，低權重
        ];

        let mut count_match = 0;
        for _ in 0..100 {
            let mut rng = thread_rng();
            let picked = pick_deception(&mut rng, &actor, &catalog);
            if picked.id == "d_match" {
                count_match += 1;
            }
        }
        // 在 100 次中，d_match 至少應該被選 50 次（85% * 9/10 ≈ 76% 期望值）
        assert!(count_match > 50, "affinity 加權未正常工作：match count = {}", count_match);
    }

    #[test]
    fn liar_is_not_always_first_actor() {
        // 跑 100 次 assemble_session，統計騙子是第一個演員的次數
        // 如果騙子真的隨機分配，不該每次都是第一個
        let actors: Vec<Actor> = (0..5)
            .map(|i| make_actor(&format!("a{i}"), (i as u8 * 2) + 1))
            .collect();
        let catalog: Vec<DeceptionPattern> = vec![
            make_deception("d1", 5),
            make_deception("d2", 8),
        ];

        let mut liar_is_first_count = 0;
        for _ in 0..100 {
            let (selected, liars, _) = assemble_session(&actors, &catalog, 3, 1).unwrap();
            // 騙子 ID 是否等於 selected 陣列的第一個演員 ID
            if liars[0] == selected[0].id {
                liar_is_first_count += 1;
            }
        }
        // 如果隨機分配，期望值 ~33%（1/3）。舊版是 100%。
        // 容許到 70 以避免極端機率誤判，但不該是 100
        assert!(
            liar_is_first_count < 70,
            "騙子排在第一位的次數 = {}，應該隨機分散而非集中在第一位",
            liar_is_first_count
        );
    }

    #[test]
    fn liar_position_varies_across_runs() {
        // 確認騙子不會永遠在同一個位置
        let actors: Vec<Actor> = (0..5)
            .map(|i| make_actor(&format!("a{i}"), (i as u8 * 2) + 1))
            .collect();
        let catalog = vec![make_deception("d1", 5)];

        let mut positions = std::collections::HashSet::new();
        for _ in 0..50 {
            let (selected, liars, _) = assemble_session(&actors, &catalog, 3, 1).unwrap();
            let pos = selected.iter().position(|a| a.id == liars[0]).unwrap();
            positions.insert(pos);
        }
        // 50 次中應該出現至少 2 種不同位置
        assert!(
            positions.len() >= 2,
            "騙子位置只出現在 {:?}，缺乏隨機性",
            positions
        );
    }
}
