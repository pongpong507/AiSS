//! # Actor Pool 模組
//!
//! 管理 InfoLit 遊戲中的演員（NPC 角色）定義與載入。
//!
//! **技術文件**：`docs/modules/actor-pool.md`

use serde::{Deserialize, Serialize};

/// 演員定義（從 YAML 載入）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub avatar: String,
    pub short_bio: String,
    #[serde(default)]
    pub personality_traits: Vec<String>,
    pub speech_style: String,
    /// 向性值 1–12（錨點 3=情感 / 6=邏輯 / 9=正式權威 / 10-12=魅力/超然型）
    pub affinity: u8,
    /// 對話積極度 1–10（越高越容易搶先發言），預設 5
    #[serde(default = "default_eagerness")]
    pub eagerness: u8,
}

fn default_eagerness() -> u8 {
    5
}

impl Actor {
    /// 驗證欄位值是否在有效範圍
    pub fn validate(&self) -> Result<(), String> {
        if self.affinity < 1 || self.affinity > 12 {
            return Err(format!(
                "演員 '{}' 的 affinity={} 不在有效範圍 [1, 12]",
                self.id, self.affinity
            ));
        }
        if self.eagerness < 1 || self.eagerness > 10 {
            return Err(format!(
                "演員 '{}' 的 eagerness={} 不在有效範圍 [1, 10]",
                self.id, self.eagerness
            ));
        }
        Ok(())
    }
}

/// 從 YAML 字串解析演員列表
pub fn parse_actors_yaml(yaml: &str) -> Result<Vec<Actor>, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

/// 從 YAML 檔案路徑載入演員列表
pub fn load_actors_from_dir(dir: &std::path::Path) -> anyhow::Result<Vec<Actor>> {
    let mut actors = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            let content = std::fs::read_to_string(&path)?;
            let actor: Actor = serde_yaml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("解析 {:?} 失敗：{}", path, e))?;
            actor.validate().map_err(|e| anyhow::anyhow!("{}", e))?;
            actors.push(actor);
        }
    }
    Ok(actors)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make(affinity: u8) -> Actor {
        Actor {
            id: "test".into(),
            name: "Test".into(),
            avatar: String::new(),
            short_bio: String::new(),
            personality_traits: vec![],
            speech_style: String::new(),
            affinity,
            eagerness: 5,
        }
    }

    #[test]
    fn validate_accepts_anchor_values() {
        for a in [3u8, 6, 9] {
            assert!(make(a).validate().is_ok(), "anchor {} should be valid", a);
        }
    }

    #[test]
    fn validate_accepts_charisma_zone() {
        for a in [10u8, 11, 12] {
            assert!(make(a).validate().is_ok(), "{} should be valid", a);
        }
    }

    #[test]
    fn validate_rejects_zero() {
        assert!(make(0).validate().is_err());
    }

    #[test]
    fn validate_rejects_above_12() {
        assert!(make(13).validate().is_err());
    }

    #[test]
    fn validate_accepts_full_range() {
        for a in 1..=12 {
            assert!(make(a).validate().is_ok(), "{} should be valid", a);
        }
    }

    #[test]
    fn parse_yaml_list() {
        let yaml = r#"
- id: "a1"
  name: "Alice"
  short_bio: "test bio"
  personality_traits: ["trait1"]
  speech_style: "casual"
  affinity: 5
- id: "a2"
  name: "Bob"
  short_bio: "another"
  personality_traits: []
  speech_style: "formal"
  affinity: 9
"#;
        let actors = parse_actors_yaml(yaml).unwrap();
        assert_eq!(actors.len(), 2);
        assert_eq!(actors[0].id, "a1");
        assert_eq!(actors[1].affinity, 9);
    }

    #[test]
    fn parse_yaml_invalid_returns_err() {
        let bad = "not valid yaml :::";
        assert!(parse_actors_yaml(bad).is_err());
    }
}
