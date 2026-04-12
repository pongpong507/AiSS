//! 演員資料匯入器
//!
//! 支援格式：YAML（列表）、CSV
//!
//! CSV 欄位（中英文欄位名皆可）：
//!   編號（id）| 名稱（name）| 簡介（short_bio）| 說話風格（speech_style）| 向性（affinity）

use super::format::{detect_format, FileFormat};
use serde::Deserialize;
use std::path::Path;
use tracing::{info, warn};

/// CSV / Excel 列的中間型別（容納中英文欄位名）
#[derive(Debug, Deserialize)]
struct ActorRow {
    // 接受中文或英文欄位名
    #[serde(alias = "編號（id）", alias = "編號", alias = "id", alias = "ID")]
    id: String,

    #[serde(alias = "名稱（name）", alias = "名稱", alias = "name")]
    name: String,

    #[serde(alias = "簡介（short_bio）", alias = "簡介", alias = "short_bio", default)]
    short_bio: String,

    #[serde(alias = "說話風格（speech_style）", alias = "說話風格", alias = "speech_style", default)]
    speech_style: String,

    #[serde(alias = "向性（affinity）", alias = "向性", alias = "affinity")]
    affinity: u8,

    #[serde(alias = "頭像（avatar）", alias = "頭像", alias = "avatar", default)]
    avatar: String,
}

/// YAML 格式（列表）
#[derive(Debug, serde::Deserialize)]
struct ActorYaml {
    id: String,
    name: String,
    #[serde(default)]
    avatar: String,
    #[serde(default)]
    short_bio: String,
    #[serde(default)]
    _personality_traits: Vec<String>,
    #[serde(default)]
    speech_style: String,
    affinity: u8,
}

pub fn run(input: &Path, out_dir: &Path, dry_run: bool) -> anyhow::Result<()> {
    let format = detect_format(input)?;
    let rows = match format {
        FileFormat::Yaml => load_yaml(input)?,
        FileFormat::Csv => load_csv(input)?,
        FileFormat::Excel => anyhow::bail!("Excel 匯入在 Milestone 0 尚未實作，請先轉成 CSV 或 YAML"),
    };

    info!("解析到 {} 筆演員資料", rows.len());

    // 驗證
    let mut errors = Vec::new();
    for row in &rows {
        if row.id.is_empty() { errors.push(format!("演員 '{}' 缺少 id", row.name)); }
        if row.name.is_empty() { errors.push(format!("id='{}' 缺少 name", row.id)); }
        if row.affinity < 1 || row.affinity > 12 {
            errors.push(format!("演員 '{}' affinity={} 不在 [1,12]", row.id, row.affinity));
        }
    }
    if !errors.is_empty() {
        for e in &errors { eprintln!("❌ 驗證錯誤：{}", e); }
        anyhow::bail!("驗證失敗（{}個錯誤），已中止匯入", errors.len());
    }

    if dry_run {
        println!("✅ Dry run 通過：{} 筆演員資料格式正確，未寫入", rows.len());
        for row in &rows {
            println!("   - {} ({}) affinity={}", row.name, row.id, row.affinity);
        }
        return Ok(());
    }

    // 寫入
    std::fs::create_dir_all(out_dir)?;
    let mut written = 0usize;
    for row in &rows {
        let out_path = out_dir.join(format!("{}.yaml", row.id));
        let yaml_content = format!(
            "id: \"{id}\"\nname: \"{name}\"\navatar: \"{avatar}\"\nshort_bio: \"{bio}\"\npersonality_traits: []\nspeech_style: \"{style}\"\naffinity: {affinity}\n",
            id = row.id,
            name = row.name,
            avatar = row.avatar,
            bio = row.short_bio,
            style = row.speech_style,
            affinity = row.affinity,
        );
        std::fs::write(&out_path, yaml_content)?;
        written += 1;
        info!("寫入 {:?}", out_path);
    }

    println!("✅ 匯入完成：{} 筆演員資料寫入 {:?}", written, out_dir);
    Ok(())
}

fn load_yaml(path: &Path) -> anyhow::Result<Vec<ActorRow>> {
    let content = std::fs::read_to_string(path)?;
    // YAML 可能是列表格式
    let actors: Vec<ActorYaml> = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("YAML 解析失敗：{}", e))?;
    Ok(actors.into_iter().map(|a| ActorRow {
        id: a.id,
        name: a.name,
        short_bio: a.short_bio,
        speech_style: a.speech_style,
        affinity: a.affinity,
        avatar: a.avatar,
    }).collect())
}

fn load_csv(path: &Path) -> anyhow::Result<Vec<ActorRow>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_path(path)?;

    let mut rows = Vec::new();
    for result in rdr.deserialize::<ActorRow>() {
        match result {
            Ok(row) => rows.push(row),
            Err(e) => warn!("CSV 列解析警告（跳過）：{}", e),
        }
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let mut p = env::temp_dir();
        p.push(format!("aiss-import-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn dry_run_yaml_passes_validation() {
        let dir = temp_dir("yaml-dry");
        let yaml_path = dir.join("input.yaml");
        fs::write(&yaml_path, r#"
- id: "actor-test"
  name: "測試演員"
  short_bio: "測試"
  speech_style: "口語"
  affinity: 5
"#).unwrap();
        let out = dir.join("out");
        let result = run(&yaml_path, &out, true);
        assert!(result.is_ok(), "dry run should succeed: {:?}", result);
        assert!(!out.exists(), "dry run should not create output");
    }

    #[test]
    fn yaml_with_invalid_affinity_fails() {
        let dir = temp_dir("invalid-affinity");
        let yaml_path = dir.join("input.yaml");
        fs::write(&yaml_path, r#"
- id: "bad"
  name: "壞演員"
  short_bio: ""
  speech_style: ""
  affinity: 99
"#).unwrap();
        let out = dir.join("out");
        let result = run(&yaml_path, &out, true);
        assert!(result.is_err());
    }

    #[test]
    fn actual_import_writes_files() {
        let dir = temp_dir("write");
        let yaml_path = dir.join("input.yaml");
        fs::write(&yaml_path, r#"
- id: "actor-1"
  name: "Alpha"
  short_bio: ""
  speech_style: ""
  affinity: 6
- id: "actor-2"
  name: "Beta"
  short_bio: ""
  speech_style: ""
  affinity: 9
"#).unwrap();
        let out = dir.join("out");
        let result = run(&yaml_path, &out, false);
        assert!(result.is_ok());
        assert!(out.join("actor-1.yaml").exists());
        assert!(out.join("actor-2.yaml").exists());
    }

    #[test]
    fn unsupported_extension_errors() {
        let dir = temp_dir("bad-ext");
        let bad_path = dir.join("input.txt");
        fs::write(&bad_path, "irrelevant").unwrap();
        let out = dir.join("out");
        let result = run(&bad_path, &out, true);
        assert!(result.is_err());
    }
}
