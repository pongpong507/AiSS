//! # Pacing 模組
//!
//! 節奏控制：模擬真實對話節奏，讓學生有時間思考。

use rand::Rng;
use serde::{Deserialize, Serialize};

/// 對話節奏設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacingConfig {
    /// 演員回應前的最短思考延遲（毫秒）
    pub min_response_delay_ms: u64,
    /// 演員回應前的最長思考延遲（毫秒）
    pub max_response_delay_ms: u64,
    /// 學生輸入的最小間隔（防狂按，毫秒）
    pub min_student_input_ms: u64,
    /// 打字機效果（字元/秒，CLI 模式可忽略）
    pub typewriter_cps: u32,
}

impl Default for PacingConfig {
    fn default() -> Self {
        Self {
            min_response_delay_ms: 1500,
            max_response_delay_ms: 3500,
            min_student_input_ms: 500,
            typewriter_cps: 30,
        }
    }
}

impl PacingConfig {
    /// 產生一個隨機回應延遲（在 min..max 之間）
    pub fn random_response_delay(&self) -> u64 {
        rand::thread_rng().gen_range(self.min_response_delay_ms..=self.max_response_delay_ms)
    }
}
