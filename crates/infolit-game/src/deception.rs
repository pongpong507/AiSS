//! # Deception Catalog 模組
//!
//! 騙術清單：定義每種「說謊的方式」及其課綱教學目標。
//!
//! **技術文件**：`docs/modules/deception-catalog.md`

use serde::{Deserialize, Serialize};
use shared_types::Difficulty;

/// 騙術模式定義（從 YAML 載入）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeceptionPattern {
    pub id: String,
    pub name_zh: String,
    pub description: String,
    #[serde(default)]
    pub example: String,
    pub difficulty: Difficulty,
    pub teaching_goal: String,
    /// 向性值 1–12，決定與哪類演員搭配的機率
    pub affinity: u8,
}

impl DeceptionPattern {
    pub fn validate(&self) -> Result<(), String> {
        if self.affinity < 1 || self.affinity > 12 {
            return Err(format!(
                "騙術 '{}' 的 affinity={} 不在有效範圍 [1, 12]",
                self.id, self.affinity
            ));
        }
        Ok(())
    }
}

/// 從 YAML 檔案目錄載入騙術清單
pub fn load_deceptions_from_dir(dir: &std::path::Path) -> anyhow::Result<Vec<DeceptionPattern>> {
    let mut patterns = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            let content = std::fs::read_to_string(&path)?;
            let pattern: DeceptionPattern = serde_yaml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("解析 {:?} 失敗：{}", path, e))?;
            pattern.validate().map_err(|e| anyhow::anyhow!("{}", e))?;
            patterns.push(pattern);
        }
    }
    Ok(patterns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::Difficulty;

    fn make(affinity: u8) -> DeceptionPattern {
        DeceptionPattern {
            id: "p1".into(),
            name_zh: "test".into(),
            description: String::new(),
            example: String::new(),
            difficulty: Difficulty::Easy,
            teaching_goal: String::new(),
            affinity,
        }
    }

    #[test]
    fn validate_rejects_out_of_range() {
        assert!(make(0).validate().is_err());
        assert!(make(13).validate().is_err());
        assert!(make(100).validate().is_err());
    }

    #[test]
    fn validate_accepts_full_range() {
        for a in 1..=12 {
            assert!(make(a).validate().is_ok());
        }
    }

    #[test]
    fn yaml_roundtrip() {
        let yaml = r#"
id: "fake-citation"
name_zh: "偽造引用"
description: "test"
example: ""
difficulty: medium
teaching_goal: "goal"
affinity: 9
"#;
        let p: DeceptionPattern = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(p.id, "fake-citation");
        assert_eq!(p.affinity, 9);
        assert_eq!(p.difficulty, Difficulty::Medium);
    }
}
