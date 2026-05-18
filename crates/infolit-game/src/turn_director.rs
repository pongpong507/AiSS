//! # TurnDirector 模組
//!
//! 統一管理「下一輪由誰發言」。
//!
//! ## 提供兩種策略
//! - [`AlgorithmicDirector`]：純演算法。權重 = `eagerness + silence_count * 2`，加權無放回抽 1-3 人
//! - [`ModeratorDirector`]：指定一個 NPC 當主持人，由 LLM 決定點誰；解析失敗時自動退到演算法
//!
//! ## 為什麼分模組
//! 原本 `session.rs` 的 `speaking_order` / `silence_count` / `silent_actors` 三個函式
//! 各做一小塊事，且行為是「每輪全員都說」造成洗版。集中到這裡並改成 1-3 人。

use crate::actor::Actor;
use crate::session::ChatTurn;
use async_trait::async_trait;
use llm_gateway::provider::LlmProvider;
use llm_gateway::types::{ChatMessage, ChatRequest};
use rand::distributions::WeightedIndex;
use rand::prelude::*;
use std::sync::Arc;
use tracing::{debug, warn};

/// 一輪挑選時所需的上下文（actor 列表 + 對話歷史）
pub struct TurnContext<'a> {
    pub actors: &'a [Actor],
    pub transcript: &'a [ChatTurn],
}

impl<'a> TurnContext<'a> {
    /// 某 actor 自上次發言以來、學生發言了幾輪
    pub fn silence_count(&self, actor_id: &str) -> u32 {
        let mut count = 0u32;
        for turn in self.transcript.iter().rev() {
            if turn.speaker_id == actor_id {
                break;
            }
            if turn.speaker_id == "student" {
                count += 1;
            }
        }
        count
    }
}

#[async_trait]
pub trait TurnDirector: Send + Sync {
    /// 回傳本輪要發言的 actor 列表，依發言順序排好
    async fn pick_next_speakers(&self, ctx: TurnContext<'_>) -> anyhow::Result<Vec<Actor>>;

    /// 此 director 是否要求某個 actor 必須在場？回傳該 actor 的 id
    fn required_actor(&self) -> Option<&str> {
        None
    }
}

// ── AlgorithmicDirector ────────────────────────────────────────────────────

/// 純演算法版：weighted-without-replacement，沉默 actor 權重加倍
pub struct AlgorithmicDirector {
    pub min_speakers: usize,
    pub max_speakers: usize,
}

impl AlgorithmicDirector {
    pub fn new(min_speakers: usize, max_speakers: usize) -> Self {
        Self { min_speakers, max_speakers }
    }
}

impl Default for AlgorithmicDirector {
    fn default() -> Self {
        Self::new(1, 3)
    }
}

#[async_trait]
impl TurnDirector for AlgorithmicDirector {
    async fn pick_next_speakers(&self, ctx: TurnContext<'_>) -> anyhow::Result<Vec<Actor>> {
        Ok(algorithmic_pick(&ctx, self.min_speakers, self.max_speakers))
    }
}

fn algorithmic_pick(ctx: &TurnContext<'_>, min: usize, max: usize) -> Vec<Actor> {
    let mut rng = thread_rng();
    let pool_size = ctx.actors.len();
    if pool_size == 0 {
        return Vec::new();
    }
    let upper = max.min(pool_size);
    let lower = min.min(upper).max(1);
    let count = rng.gen_range(lower..=upper);

    // 找到最後一位非學生的發言者，避免連續輪由同一人說話
    let last_speaker_id: Option<&str> = ctx
        .transcript
        .iter()
        .rev()
        .find(|t| t.speaker_id != "student")
        .map(|t| t.speaker_id.as_str());

    let mut pool: Vec<Actor> = ctx.actors.to_vec();
    let mut picked: Vec<Actor> = Vec::with_capacity(count);
    for _ in 0..count {
        if pool.is_empty() {
            break;
        }
        let weights: Vec<f64> = pool
            .iter()
            .map(|a| {
                let silence = ctx.silence_count(&a.id) as f64;
                let base = (a.eagerness as f64) + silence * 2.0;
                // 上一位剛說過話的人權重降到 5%，避免連續發言洗版
                if Some(a.id.as_str()) == last_speaker_id {
                    base * 0.05
                } else {
                    base
                }
            })
            .collect();
        let dist = match WeightedIndex::new(&weights) {
            Ok(d) => d,
            Err(_) => break,
        };
        let idx = dist.sample(&mut rng);
        picked.push(pool.remove(idx));
    }
    picked
}

// ── ModeratorDirector ──────────────────────────────────────────────────────

/// 由特定 NPC 當主持人，LLM 決定點誰；任何錯誤都 fallback 到 algorithmic
pub struct ModeratorDirector {
    pub moderator_id: String,
    pub min_speakers: usize,
    pub max_speakers: usize,
    pub provider: Arc<dyn LlmProvider>,
    pub model: String,
}

impl ModeratorDirector {
    pub fn new(
        moderator_id: String,
        provider: Arc<dyn LlmProvider>,
        model: String,
        min_speakers: usize,
        max_speakers: usize,
    ) -> Self {
        Self { moderator_id, min_speakers, max_speakers, provider, model }
    }
}

#[async_trait]
impl TurnDirector for ModeratorDirector {
    fn required_actor(&self) -> Option<&str> {
        Some(&self.moderator_id)
    }

    async fn pick_next_speakers(&self, ctx: TurnContext<'_>) -> anyhow::Result<Vec<Actor>> {
        let moderator = match ctx.actors.iter().find(|a| a.id == self.moderator_id) {
            Some(a) => a.clone(),
            None => {
                warn!(moderator = %self.moderator_id, "主持人不在本場 actor 中，退回演算法");
                return Ok(algorithmic_pick(&ctx, self.min_speakers, self.max_speakers));
            }
        };

        // 主持人本輪是否要發言？
        // - 開場（transcript 空）→ 說，當主持人開場
        // - 最近一筆是學生 → 說，回應/引導學生
        // - 主持人沉默至少 2 個學生回合 → 補一句免得完全消失
        // - 其他情況 → 只主持，不發言
        let moderator_speaks = should_moderator_speak(&ctx, &moderator);

        let mut result: Vec<Actor> = if moderator_speaks {
            vec![moderator.clone()]
        } else {
            Vec::new()
        };

        let extras_max = self.max_speakers.saturating_sub(result.len());
        let extras_min = if moderator_speaks { 0 } else { 1 };

        if extras_max > 0 {
            let prompt = build_moderator_prompt(&moderator, &ctx, extras_min, extras_max);
            let req = ChatRequest::new(
                &self.model,
                vec![ChatMessage::system(&prompt)],
            )
            .with_temperature(0.5)
            .with_max_tokens(80);

            let others = match self.provider.chat(req).await {
                Ok(response) => {
                    debug!(content = %response.content, "主持人挑人回應");
                    let mut picked = parse_moderator_response(
                        &response.content,
                        ctx.actors,
                        extras_max,
                    );
                    picked.retain(|a| a.id != self.moderator_id);
                    picked
                }
                Err(e) => {
                    warn!(error = %e, "主持人 LLM 呼叫失敗，改用演算法挑其他人");
                    algorithmic_pick(&ctx, extras_min.max(1), extras_max)
                        .into_iter()
                        .filter(|a| a.id != self.moderator_id)
                        .take(extras_max)
                        .collect()
                }
            };
            result.extend(others);
        }

        result.truncate(self.max_speakers);
        // safety：萬一所有條件都湊不出人，退回 algorithmic 至少給 1 人
        if result.is_empty() {
            return Ok(algorithmic_pick(&ctx, 1, self.max_speakers));
        }
        Ok(result)
    }
}

/// 決定主持人本輪是否該開口
fn should_moderator_speak(ctx: &TurnContext<'_>, moderator: &Actor) -> bool {
    // 開場：transcript 空 → 說
    if ctx.transcript.is_empty() {
        return true;
    }
    // 學生剛說話 → 主持人引導 / 回應
    if ctx
        .transcript
        .last()
        .map(|t| t.speaker_id.as_str())
        == Some("student")
    {
        return true;
    }
    // 已經至少 2 個學生回合沒講話 → 主持人補一句，避免完全消失
    if ctx.silence_count(&moderator.id) >= 2 {
        return true;
    }
    false
}

fn build_moderator_prompt(
    moderator: &Actor,
    ctx: &TurnContext<'_>,
    min: usize,
    max: usize,
) -> String {
    let others_names: Vec<&str> = ctx
        .actors
        .iter()
        .filter(|a| a.id != moderator.id)
        .map(|a| a.name.as_str())
        .collect();
    let names_csv = others_names.join("、");

    let recent_lines: Vec<String> = ctx
        .transcript
        .iter()
        .rev()
        .take(12)
        .rev()
        .map(|t| format!("{}：{}", t.speaker_name, t.content))
        .collect();
    let recent = if recent_lines.is_empty() {
        "（尚無對話）".to_string()
    } else {
        recent_lines.join("\n")
    };

    let last_speaker: Option<&str> = ctx
        .transcript
        .iter()
        .rev()
        .find(|t| t.speaker_id != "student" && t.speaker_id != moderator.id)
        .map(|t| t.speaker_name.as_str());
    let avoid_hint = match last_speaker {
        Some(name) => format!("剛才講過話的是「{}」，這次盡量點別人，給大家輪流的機會。", name),
        None => String::new(),
    };

    let silent_list: Vec<&str> = ctx
        .actors
        .iter()
        .filter(|a| a.id != moderator.id && ctx.silence_count(&a.id) >= 2)
        .map(|a| a.name.as_str())
        .collect();
    let silent_hint = if silent_list.is_empty() {
        "（無）".to_string()
    } else {
        silent_list.join("、")
    };

    format!(
        "你是「{name}」，這場討論的主持人。你接下來會先發言一次，然後再點別人發言。\n\
         其他在場的人：{names}\n\
         最近的對話：\n{recent}\n\n\
         長期沒發言的人：{silent}\n\
         {avoid}\n\n\
         請從上面「其他在場的人」中，挑 {min} 到 {max} 個你想邀請發言的人。\n\
         只輸出名字，用半形逗號分隔，不要加任何其他文字或解釋。\n\
         範例：張三, 李四\n",
        name = moderator.name,
        names = names_csv,
        recent = recent,
        silent = silent_hint,
        avoid = avoid_hint,
        min = min,
        max = max,
    )
}

fn parse_moderator_response(raw: &str, actors: &[Actor], max: usize) -> Vec<Actor> {
    let cleaned = raw.trim();
    let tokens: Vec<&str> = cleaned
        .split(|c: char| c == ',' || c == '，' || c == '、' || c == '\n')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let mut picked: Vec<Actor> = Vec::new();
    for token in tokens {
        let stripped: String = token
            .chars()
            .filter(|c| !matches!(c, '[' | ']' | '「' | '」' | '『' | '』' | '"' | '\''))
            .collect();
        let stripped = stripped.trim();
        if stripped.is_empty() {
            continue;
        }
        if let Some(actor) = actors.iter().find(|a| a.name == stripped || a.id == stripped) {
            if !picked.iter().any(|p| p.id == actor.id) {
                picked.push(actor.clone());
                if picked.len() >= max {
                    break;
                }
            }
        }
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deception::DeceptionPattern;
    use async_trait::async_trait;
    use llm_gateway::error::LlmError;
    use llm_gateway::provider::{ChatStream, LlmProvider};
    use llm_gateway::types::{ChatResponse, Embedding, ProviderCapabilities};
    use shared_types::Difficulty;
    use std::collections::HashMap;

    fn mk_actor(id: &str, name: &str, eagerness: u8) -> Actor {
        Actor {
            id: id.into(),
            name: name.into(),
            avatar: String::new(),
            short_bio: format!("{name} 的簡介"),
            personality_traits: vec!["特質".into()],
            speech_style: "中性".into(),
            discussion_lens: String::new(),
            affinity: 5,
            eagerness,
        }
    }

    fn mk_actors() -> Vec<Actor> {
        vec![
            mk_actor("a1", "甲", 5),
            mk_actor("a2", "乙", 5),
            mk_actor("a3", "丙", 5),
            mk_actor("a4", "丁", 5),
            mk_actor("a5", "戊", 5),
        ]
    }

    fn mk_turn(speaker_id: &str, name: &str) -> ChatTurn {
        ChatTurn {
            speaker_id: speaker_id.into(),
            speaker_name: name.into(),
            content: "...".into(),
            is_liar: false,
        }
    }

    #[tokio::test]
    async fn algorithmic_picks_within_bounds() {
        let actors = mk_actors();
        let dir = AlgorithmicDirector::new(1, 3);
        for _ in 0..30 {
            let ctx = TurnContext { actors: &actors, transcript: &[] };
            let picks = dir.pick_next_speakers(ctx).await.unwrap();
            assert!(picks.len() >= 1 && picks.len() <= 3, "got {}", picks.len());
        }
    }

    #[tokio::test]
    async fn algorithmic_no_duplicates() {
        let actors = mk_actors();
        let dir = AlgorithmicDirector::new(3, 3);
        for _ in 0..30 {
            let ctx = TurnContext { actors: &actors, transcript: &[] };
            let picks = dir.pick_next_speakers(ctx).await.unwrap();
            let mut ids: Vec<&str> = picks.iter().map(|a| a.id.as_str()).collect();
            ids.sort();
            ids.dedup();
            assert_eq!(ids.len(), picks.len(), "重複了：{:?}", picks);
        }
    }

    #[tokio::test]
    async fn algorithmic_avoids_back_to_back_same_speaker() {
        let actors = mk_actors();
        // a1 是最後一位非學生發言者
        let transcript = vec![mk_turn("student", "你"), mk_turn("a1", "甲")];
        let dir = AlgorithmicDirector::new(1, 1);
        let mut a1_count = 0u32;
        for _ in 0..400 {
            let ctx = TurnContext { actors: &actors, transcript: &transcript };
            let picks = dir.pick_next_speakers(ctx).await.unwrap();
            if picks.first().map(|a| a.id.as_str()) == Some("a1") {
                a1_count += 1;
            }
        }
        // 預期 a1 被連續挑到的機率遠低於均勻 20%；保守上界 8%
        assert!(
            a1_count < 32,
            "a1 不該連續被選太多次：實際 {} / 400",
            a1_count
        );
    }

    #[tokio::test]
    async fn algorithmic_prefers_silent_actors() {
        // 構造 transcript：a1 講過很多次、a2 很久沒講
        // 中間插學生發言來推高 a2 的 silence_count
        let actors = mk_actors();
        let mut transcript: Vec<ChatTurn> = Vec::new();
        transcript.push(mk_turn("a1", "甲"));
        transcript.push(mk_turn("a2", "乙"));
        for _ in 0..4 {
            transcript.push(mk_turn("student", "你"));
            transcript.push(mk_turn("a1", "甲"));
        }

        let dir = AlgorithmicDirector::new(1, 1);
        let mut a1_count = 0u32;
        let mut a2_count = 0u32;
        for _ in 0..400 {
            let ctx = TurnContext { actors: &actors, transcript: &transcript };
            let picks = dir.pick_next_speakers(ctx).await.unwrap();
            if let Some(p) = picks.first() {
                match p.id.as_str() {
                    "a1" => a1_count += 1,
                    "a2" => a2_count += 1,
                    _ => {}
                }
            }
        }
        assert!(
            a2_count > a1_count,
            "沉默者 a2 應比常發言的 a1 更常被選：a1={}, a2={}",
            a1_count,
            a2_count
        );
    }

    // ── Moderator tests ──

    struct StubProvider {
        reply: String,
    }

    #[async_trait]
    impl LlmProvider for StubProvider {
        fn name(&self) -> &'static str { "stub" }
        fn capabilities(&self) -> ProviderCapabilities { ProviderCapabilities::default() }
        async fn chat(&self, _: ChatRequest) -> Result<ChatResponse, LlmError> {
            Ok(ChatResponse {
                content: self.reply.clone(),
                model: "stub".into(),
                usage: None,
                extensions: HashMap::new(),
            })
        }
        async fn chat_stream(&self, _: ChatRequest) -> Result<ChatStream, LlmError> {
            Err(LlmError::UnsupportedCapability("stub no stream".into()))
        }
        async fn embed(&self, _: &[String]) -> Result<Vec<Embedding>, LlmError> { Ok(vec![]) }
    }

    fn mk_moderator_dir(reply: &str) -> ModeratorDirector {
        let provider: Arc<dyn LlmProvider> = Arc::new(StubProvider { reply: reply.into() });
        ModeratorDirector::new("a1".into(), provider, "stub-model".into(), 1, 3)
    }

    #[tokio::test]
    async fn moderator_parses_valid_response() {
        // 主持人是 a1，LLM 回 乙(a2)、丙(a3) → 結果應是 [a1, a2, a3]，主持人優先
        let actors = mk_actors();
        let dir = mk_moderator_dir("乙, 丙");
        let ctx = TurnContext { actors: &actors, transcript: &[] };
        let picks = dir.pick_next_speakers(ctx).await.unwrap();
        assert_eq!(picks.len(), 3);
        assert_eq!(picks[0].id, "a1", "主持人應在第一位");
        let other_ids: Vec<&str> = picks[1..].iter().map(|a| a.id.as_str()).collect();
        assert!(other_ids.contains(&"a2"));
        assert!(other_ids.contains(&"a3"));
    }

    #[tokio::test]
    async fn moderator_always_speaks() {
        // 即使 LLM 回應一堆無法解析的字，主持人也應在輸出中
        let actors = mk_actors();
        let dir = mk_moderator_dir("不確定誰要說 抱歉");
        let ctx = TurnContext { actors: &actors, transcript: &[] };
        let picks = dir.pick_next_speakers(ctx).await.unwrap();
        assert!(!picks.is_empty());
        assert_eq!(picks[0].id, "a1", "主持人應在第一位");
    }

    #[tokio::test]
    async fn moderator_handles_unknown_names() {
        // 「不存在的人」應被略過，剩下「丁」仍然有效；主持人優先在前
        let actors = mk_actors();
        let dir = mk_moderator_dir("不存在的人, 丁");
        let ctx = TurnContext { actors: &actors, transcript: &[] };
        let picks = dir.pick_next_speakers(ctx).await.unwrap();
        assert_eq!(picks.len(), 2);
        assert_eq!(picks[0].id, "a1");
        assert_eq!(picks[1].id, "a4");
    }

    #[tokio::test]
    async fn moderator_drops_self_from_llm_picks() {
        // LLM 不應點主持人自己（prompt 已排除她），但若意外點到也要過濾
        let actors = mk_actors();
        let dir = mk_moderator_dir("甲, 乙");
        let ctx = TurnContext { actors: &actors, transcript: &[] };
        let picks = dir.pick_next_speakers(ctx).await.unwrap();
        let a1_count = picks.iter().filter(|a| a.id == "a1").count();
        assert_eq!(a1_count, 1, "主持人不應出現兩次");
    }

    #[tokio::test]
    async fn moderator_silent_when_other_npc_just_spoke() {
        // 沒學生發言、最近講話的是其他 NPC、主持人剛講過 → 不該再說
        let actors = mk_actors();
        let transcript = vec![
            mk_turn("a1", "甲"), // moderator 剛說過
            mk_turn("a2", "乙"), // 其他人說
        ];
        let dir = mk_moderator_dir("丁");
        let ctx = TurnContext { actors: &actors, transcript: &transcript };
        let picks = dir.pick_next_speakers(ctx).await.unwrap();
        // 主持人 a1 不該在結果中
        assert!(
            picks.iter().all(|a| a.id != "a1"),
            "主持人剛說過就不該再說：{:?}",
            picks.iter().map(|a| a.id.as_str()).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn moderator_speaks_after_student_question() {
        // 學生剛提問 → 主持人應發言引導
        let actors = mk_actors();
        let transcript = vec![
            mk_turn("a1", "甲"),
            mk_turn("a2", "乙"),
            mk_turn("student", "你"),
        ];
        let dir = mk_moderator_dir("丙");
        let ctx = TurnContext { actors: &actors, transcript: &transcript };
        let picks = dir.pick_next_speakers(ctx).await.unwrap();
        assert!(
            picks.iter().any(|a| a.id == "a1"),
            "學生提問後主持人應發言"
        );
    }

    #[tokio::test]
    async fn moderator_breaks_long_silence() {
        // 主持人沉默 ≥ 2 學生回合 → 應補一句
        let actors = mk_actors();
        let mut transcript = vec![mk_turn("a1", "甲")]; // 主持人開場
        for _ in 0..2 {
            transcript.push(mk_turn("student", "你"));
            transcript.push(mk_turn("a2", "乙"));
        }
        let dir = mk_moderator_dir("丙");
        let ctx = TurnContext { actors: &actors, transcript: &transcript };
        let picks = dir.pick_next_speakers(ctx).await.unwrap();
        assert!(
            picks.iter().any(|a| a.id == "a1"),
            "主持人久未發言應補上：{:?}",
            picks.iter().map(|a| a.id.as_str()).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn moderator_missing_moderator_falls_back() {
        let actors = mk_actors();
        // moderator_id 是 a1，actors 中故意拿掉 a1
        let provider: Arc<dyn LlmProvider> = Arc::new(StubProvider { reply: "丁".into() });
        let dir = ModeratorDirector::new("nonexistent".into(), provider, "stub".into(), 1, 3);
        let ctx = TurnContext { actors: &actors, transcript: &[] };
        let picks = dir.pick_next_speakers(ctx).await.unwrap();
        assert!(!picks.is_empty(), "應 fallback 給結果");
    }

    // 防 unused warning
    fn _check_deception_type(_d: DeceptionPattern, _diff: Difficulty) {}
}
