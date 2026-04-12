//! 騙術資料匯入器
//!
//! CSV 欄位（中英文欄位名皆可）：
//!   編號（id）| 名稱（name_zh）| 難度（difficulty）| 向性（affinity）| 教學目標（teaching_goal）

use super::format::{detect_format, FileFormat};
use serde::Deserialize;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
struct DeceptionRow {
    #[serde(alias = "編號（id）", alias = "編號", alias = "id", alias = "ID")]
    id: String,

    #[serde(alias = "名稱（name_zh）", alias = "名稱", alias = "name_zh")]
    name_zh: String,

    #[serde(alias = "說明（description）", alias = "說明", alias = "description", default)]
    description: String,

    #[serde(alias = "難度（difficulty）", alias = "難度", alias = "difficulty", default)]
    difficulty: String,

    #[serde(alias = "向性（affinity）", alias = "向性", alias = "affinity")]
    affinity: u8,

    #[serde(alias = "教學目標（teaching_goal）", alias = "教學目標", alias = "teaching_goal", default)]
    teaching_goal: String,

    #[serde(alias = "範例（example）", alias = "範例", alias = "example", default)]
    example: String,
}

#[derive(Debug, serde::Deserialize)]
struct DeceptionYaml {
    id: String,
    name_zh: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    difficulty: String,
    affinity: u8,
    #[serde(default)]
    teaching_goal: String,
    #[serde(default)]
    example: String,
}

pub fn run(input: &Path, out_dir: &Path, dry_run: bool) -> anyhow::Result<()> {
    let format = detect_format(input)?;
    let rows = match format {
        FileFormat::Yaml => load_yaml(input)?,
        FileFormat::Csv => load_csv(input)?,
        FileFormat::Excel => anyhow::bail!("Excel 匯入在 Milestone 0 尚未實作，請先轉成 CSV 或 YAML"),
    };

    info!("解析到 {} 筆騙術資料", rows.len());

    // 驗證
    let valid_difficulties = ["easy", "medium", "hard", "易", "中", "難"];
    let mut errors = Vec::new();
    for row in &rows {
        if row.id.is_empty() { errors.push(format!("騙術 '{}' 缺少 id", row.name_zh)); }
        if row.affinity < 1 || row.affinity > 12 {
            errors.push(format!("騙術 '{}' affinity={} 不在 [1,12]", row.id, row.affinity));
        }
        let diff_lower = row.difficulty.to_lowercase();
        if !row.difficulty.is_empty() && !valid_difficulties.contains(&diff_lower.as_str()) {
            errors.push(format!("騙術 '{}' difficulty='{}' 不合法（易/中/難 或 easy/medium/hard）", row.id, row.difficulty));
        }
    }
    if !errors.is_empty() {
        for e in &errors { eprintln!("❌ 驗證錯誤：{}", e); }
        anyhow::bail!("驗證失敗（{}個錯誤），已中止匯入", errors.len());
    }

    if dry_run {
        println!("✅ Dry run 通過：{} 筆騙術資料格式正確，未寫入", rows.len());
        for row in &rows {
            println!("   - {} ({}) affinity={} 難度={}", row.name_zh, row.id, row.affinity, row.difficulty);
        }
        return Ok(());
    }

    // 難度正規化（中文 → 英文）
    fn normalize_difficulty(s: &str) -> &str {
        match s {
            "易" | "easy" | "Easy" => "easy",
            "難" | "hard" | "Hard" => "hard",
            _ => "medium",
        }
    }

    std::fs::create_dir_all(out_dir)?;
    let mut written = 0usize;
    for row in &rows {
        let out_path = out_dir.join(format!("{}.yaml", row.id));
        let yaml_content = format!(
            "id: \"{id}\"\nname_zh: \"{name_zh}\"\ndescription: \"{desc}\"\nexample: \"{example}\"\ndifficulty: {difficulty}\nteaching_goal: \"{goal}\"\naffinity: {affinity}\n",
            id = row.id,
            name_zh = row.name_zh,
            desc = row.description,
            example = row.example,
            difficulty = normalize_difficulty(&row.difficulty),
            goal = row.teaching_goal,
            affinity = row.affinity,
        );
        std::fs::write(&out_path, yaml_content)?;
        written += 1;
        info!("寫入 {:?}", out_path);
    }

    println!("✅ 匯入完成：{} 筆騙術資料寫入 {:?}", written, out_dir);
    Ok(())
}

fn load_yaml(path: &Path) -> anyhow::Result<Vec<DeceptionRow>> {
    let content = std::fs::read_to_string(path)?;
    let items: Vec<DeceptionYaml> = serde_yaml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("YAML 解析失敗：{}", e))?;
    Ok(items.into_iter().map(|d| DeceptionRow {
        id: d.id,
        name_zh: d.name_zh,
        description: d.description,
        difficulty: d.difficulty,
        affinity: d.affinity,
        teaching_goal: d.teaching_goal,
        example: d.example,
    }).collect())
}

fn load_csv(path: &Path) -> anyhow::Result<Vec<DeceptionRow>> {
    let mut rdr = csv::ReaderBuilder::new().has_headers(true).flexible(true).from_path(path)?;
    let mut rows = Vec::new();
    for result in rdr.deserialize::<DeceptionRow>() {
        match result {
            Ok(row) => rows.push(row),
            Err(e) => warn!("CSV 列解析警告（跳過）：{}", e),
        }
    }
    Ok(rows)
}
