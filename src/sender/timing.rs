//! 变速回复系统
//!
//! 将固定的打字延迟改为动态的、受多种因素影响的回复节奏。
//! - 回复延迟 = 基础延迟 + 内容长度 + 随机波动 + 认知复杂度延迟
//! - 对简单问题（"嗯""好"）延迟极短
//! - 长回复有概率拆成"先发简短反应，再发完整内容"

use crate::config;

/// 回复时机配置（动态计算）
pub struct ResponseTiming {
    /// 基础打字速度（字符/秒），受人格影响
    pub base_typing_speed: f32,
    /// 当前修正系数（受精力/情绪/昼夜节律影响）
    pub speed_modifier: f32,
    /// 回复前额外等待（模拟思考）的概率
    pub thinking_pause_probability: f32,
}

impl Default for ResponseTiming {
    fn default() -> Self {
        let cfg = config::get();
        let h = &cfg.humanity;
        Self {
            base_typing_speed: h.base_typing_speed,
            speed_modifier: 1.0,
            thinking_pause_probability: h.thinking_pause_probability,
        }
    }
}

impl ResponseTiming {
    /// 根据当前状态计算修正后的打字速度
    pub fn effective_speed(&self) -> f32 {
        (self.base_typing_speed * self.speed_modifier).max(1.0)
    }

    /// 计算回复延迟（毫秒）
    ///
    /// 延迟 = 基础延迟 + 内容长度/打字速度 + 随机波动 + 思考暂停
    pub fn calculate_delay(&self, reply_text: &str) -> u64 {
        let char_count = reply_text.chars().count() as f32;
        let cfg = config::get();

        // 基础打字延迟
        let typing_delay = (char_count / self.effective_speed() * 1000.0) as u64;

        // 对极短回复（"嗯""好""哦"等），延迟极短
        if char_count <= 2.0 {
            return typing_delay.clamp(100, 500);
        }

        // 思考暂停：有概率添加额外延迟（模拟思考时间）
        let thinking_delay = if fastrand::f32() < self.thinking_pause_probability {
            fastrand::u64(500..3000)
        } else {
            0
        };

        // 随机波动（±30%）
        let jitter_factor = 0.7 + fastrand::f32() * 0.6;
        let total = ((typing_delay as f32 * jitter_factor) as u64 + thinking_delay)
            .min(cfg.conversation.max_typing_delay_ms);

        total.max(200)
    }

    /// 根据当前状态更新修正系数
    pub fn update_modifiers(
        &mut self,
        battery_level: f32,
        circadian_energy: f32,
        attention_level: f32,
    ) {
        // 电量低 → 打字变慢
        let battery_mod = 0.6 + battery_level * 0.4;

        // 昼夜节律精力低 → 打字变慢
        let circadian_mod = 0.7 + circadian_energy * 0.3;

        // 注意力低 → 打字变慢（心不在焉）
        let attention_mod = 0.8 + attention_level * 0.2;

        self.speed_modifier = (battery_mod * circadian_mod * attention_mod).clamp(0.3, 1.5);
    }
}
