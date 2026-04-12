//! # Topic 模組
//!
//! 從 YAML 檔案載入討論題庫。
//!
//! 題庫存放在 `content/topics/` 目錄，每個 `.yaml` 檔案代表一個題目。
//! 未來可改為從資料庫讀取，只需替換此模組的載入函數即可。

use shared_types::Topic;

/// 從目錄載入所有題目
pub fn load_topics_from_dir(dir: &std::path::Path) -> anyhow::Result<Vec<Topic>> {
    let mut topics = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
            let content = std::fs::read_to_string(&path)?;
            let topic: Topic = serde_yaml::from_str(&content)
                .map_err(|e| anyhow::anyhow!("解析 {:?} 失敗：{}", path, e))?;
            topics.push(topic);
        }
    }
    if topics.is_empty() {
        anyhow::bail!("題庫目錄 {:?} 中沒有找到任何 .yaml 檔案", dir);
    }
    Ok(topics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_from_temp_dir() {
        let dir = std::env::temp_dir().join("aiss_topic_test");
        let _ = std::fs::create_dir_all(&dir);

        let yaml = r#"
id: "t-test"
question: "測試題目？"
correct_answer: "測試答案"
difficulty: easy
tags:
  - "測試"
"#;
        let path = dir.join("t-test.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(yaml.as_bytes()).unwrap();

        let topics = load_topics_from_dir(&dir).unwrap();
        assert!(!topics.is_empty());
        assert_eq!(topics[0].id, "t-test");

        // cleanup
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn empty_dir_returns_error() {
        let dir = std::env::temp_dir().join("aiss_topic_test_empty");
        let _ = std::fs::create_dir_all(&dir);
        // 確保目錄是空的
        for entry in std::fs::read_dir(&dir).unwrap() {
            let _ = std::fs::remove_file(entry.unwrap().path());
        }
        let result = load_topics_from_dir(&dir);
        assert!(result.is_err());
    }
}
