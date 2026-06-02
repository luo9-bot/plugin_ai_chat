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
    /// 长回复拆成两条的概率
    pub split_reply_probability: f32,
}

impl Default for ResponseTiming {
    fn default() -> Self {
        let cfg = config::get();
        let h = &cfg.humanity;
        Self {
            base_typing_speed: h.base_typing_speed,
            speed_modifier: 1.0,
            thinking_pause_probability: h.thinking_pause_probability,
            split_reply_probability: h.split_reply_probability,
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
            return (typing_delay.min(500)).max(100);
        }

        // 思考暂停：有概率添加额外延迟（模拟思考时间）
        let thinking_delay = if fastrand::f32() < self.thinking_pause_probability {
            let pause_secs = fastrand::u64(500..3000);
            pause_secs
        } else {
            0
        };

        // 随机波动（±30%）
        let jitter_factor = 0.7 + fastrand::f32() * 0.6;
        let total = ((typing_delay as f32 * jitter_factor) as u64 + thinking_delay)
            .min(cfg.conversation.max_typing_delay_ms);

        total.max(200)
    }

    /// 判断是否应该将长回复拆成两条
    pub fn should_split_reply(&self, reply_text: &str) -> bool {
        let char_count = reply_text.chars().count();
        if char_count < 30 {
            return false; // 太短不拆
        }
        // 越长越可能拆
        let base_prob = self.split_reply_probability;
        let length_bonus = ((char_count - 30) as f32 / 100.0).min(0.3);
        fastrand::f32() < (base_prob + length_bonus)
    }

    /// 拆分回复："先发简短反应，再发完整内容"
    pub fn split_reply(reply_text: &str) -> Option<(String, String)> {
        let chars: Vec<char> = reply_text.chars().collect();
        if chars.len() < 20 {
            return None;
        }

        // 找第一个句子边界（句号、换行、逗号）
        let split_points: Vec<usize> = chars.iter().enumerate()
            .filter(|(_, c)| matches!(c, '。' | '\n' | '！' | '？' | '…'))
            .map(|(i, _)| i + 1)
            .filter(|&i| i >= 5 && i <= chars.len() / 2)
            .collect();

        if let Some(&point) = split_points.first() {
            let first: String = chars[..point].iter().collect();
            let second: String = chars[point..].iter().collect();
            if first.len() >= 3 && second.len() >= 3 {
                return Some((first, second));
            }
        }

        // 备选：在第一个逗号处拆分
        if let Some(pos) = chars.iter().position(|&c| c == '，' || c == ',') {
            if pos >= 5 && pos <= chars.len() - 5 {
                let first: String = chars[..pos + 1].iter().collect();
                let second: String = chars[pos + 1..].iter().collect();
                return Some((first, second));
            }
        }

        None
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

/// 为回复添加人性扰动
///
/// - 根据注意力水平调整回复长度
/// - 根据电量调整语气活力
/// - 小概率添加口头禅
/// - 小概率自我纠正
pub fn apply_humanity_filter(
    reply: &str,
    attention_level: f32,
    battery_level: f32,
) -> String {
    let cfg = config::get();
    let h = &cfg.humanity;

    if !h.humanity_filter_enabled {
        return reply.to_string();
    }

    let mut result = reply.to_string();

    // 低注意力时自然缩短回复
    if attention_level < 0.3 {
        // 截断到前1-2句
        let sentences: Vec<&str> = result.split_inclusive(&['。', '！', '？', '…', '\n'][..])
            .collect();
        if sentences.len() > 2 {
            result = sentences[..2].join("");
        }
    }

    // 小概率添加口头禅
    if fastrand::f32() < h.catchphrase_probability {
        let catchphrases = [
            "就是说", "嗯…", "啊这", "害", "就很", "怎么说呢",
            "大概", "反正", "嘛", "吧",
        ];
        let idx = fastrand::usize(0..catchphrases.len());
        // 在开头添加
        if fastrand::f32() < 0.5 {
            result = format!("{}，{}", catchphrases[idx], result);
        }
    }

    // 低电量时语气更平淡（减少感叹号和表情相关表达）
    if battery_level < 0.3 {
        result = result.replace('！', "。");
        result = result.replace("哈哈哈", "嗯");
        result = result.replace("哈哈", "");
    }

    result
}
