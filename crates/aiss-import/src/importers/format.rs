//! 格式偵測與通用解析工具

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileFormat {
    Yaml,
    Csv,
    Excel,
}

pub fn detect_format(path: &Path) -> anyhow::Result<FileFormat> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("yaml") | Some("yml") => Ok(FileFormat::Yaml),
        Some("csv") => Ok(FileFormat::Csv),
        Some("xlsx") | Some("xls") => Ok(FileFormat::Excel),
        other => anyhow::bail!("不支援的檔案格式：{:?}（支援：yaml / csv / xlsx）", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_yaml() {
        assert_eq!(detect_format(&PathBuf::from("foo.yaml")).unwrap(), FileFormat::Yaml);
        assert_eq!(detect_format(&PathBuf::from("foo.yml")).unwrap(), FileFormat::Yaml);
    }

    #[test]
    fn detects_csv() {
        assert_eq!(detect_format(&PathBuf::from("data.csv")).unwrap(), FileFormat::Csv);
    }

    #[test]
    fn detects_excel() {
        assert_eq!(detect_format(&PathBuf::from("a.xlsx")).unwrap(), FileFormat::Excel);
        assert_eq!(detect_format(&PathBuf::from("a.xls")).unwrap(), FileFormat::Excel);
    }

    #[test]
    fn rejects_unknown_extension() {
        assert!(detect_format(&PathBuf::from("foo.txt")).is_err());
        assert!(detect_format(&PathBuf::from("noext")).is_err());
    }
}
