//! 变速回复系统
//!
//! 回复节奏不是一条公式：真人有时秒回、有时想了半天才打字、
//! 打字途中还会停一下。因此延迟由三部分组成：
//!
//! - **读**：看一眼对方说了什么（随对方消息长度增长，有上限）
//! - **想**：决定说什么。这一项是**指数分布**——大多数时候很快，
//!   偶尔很长，正是"长尾"手感。固定概率的常数停顿做不到这一点。
//! - **打**：按字数除速度，每个字的速度本身还有波动
//!
//! 旧实现只有"字数 ÷ 速度 × 均匀抖动(±30%) + 15% 概率加常数"，
//! 实测 49% 的相邻发送间隔挤在 1/2/3/4 秒四个档位上，且
//! "5~9 字 → 4 秒"重复出现 22 次：这种整齐本身就是机械感。

use crate::config;

/// 读一眼所需的时间上限（毫秒）：对方写得再长也不会读一分钟
const MAX_READ_MS: u64 = 2500;

/// 回复时机配置（动态计算）
pub struct ResponseTiming {
    /// 基础打字速度（字符/秒），受人格影响
    pub base_typing_speed: f32,
    /// 当前修正系数（受精力/情绪/昼夜节律影响）
    pub speed_modifier: f32,
}

impl Default for ResponseTiming {
    fn default() -> Self {
        let cfg = config::get();
        let h = &cfg.humanity;
        Self {
            base_typing_speed: h.base_typing_speed,
            speed_modifier: 1.0,
        }
    }
}

impl ResponseTiming {
    /// 根据当前状态计算修正后的打字速度
    pub fn effective_speed(&self) -> f32 {
        (self.base_typing_speed * self.speed_modifier).max(1.0)
    }

    /// 指数分布采样：均值 `mean_ms`，长尾截断在 `cap_ms`
    ///
    /// 用逆变换 `-mean·ln(u)`：多数取值远小于均值，少数取值很大。
    /// 这正是"想一下"该有的形状——固定概率的常数停顿会同时丢掉
    /// "秒回"和"想很久"两头。
    fn exponential_ms(mean_ms: f32, cap_ms: f32) -> u64 {
        let u = fastrand::f32().max(f32::EPSILON);
        let sample = -mean_ms * u.ln();
        sample.min(cap_ms) as u64
    }

    /// 计算第一条消息发出前的延迟（毫秒）
    ///
    /// `incoming` 是刚看到的消息内容（用来估算"读完"的时间），
    /// `cap_ms` 是配置里的延迟上限。延迟上限由调用方给出，本函数
    /// 不读配置——这样节奏计算是纯函数，可测且不受全局状态影响。
    pub fn calculate_delay(&self, reply_text: &str, incoming: &str, cap_ms: u64) -> u64 {
        let reply_chars = reply_text.chars().count() as f32;
        let incoming_chars = incoming.chars().count() as f32;
        let cap = cap_ms.max(1);

        // 极短回复（嗯/好/哦）几乎不假思索：真人这类回应是条件反射
        if reply_chars <= 3.0 {
            return fastrand::u64(120..=450).min(cap);
        }

        let read_ms = (incoming_chars * 18.0).min(MAX_READ_MS as f32);

        // 思考的期望值随回复长度增长：话越多越要先想清楚
        let think_mean = 220.0 + reply_chars * 22.0;
        let think_ms = Self::exponential_ms(think_mean, cap as f32 * 0.6);

        // 打字：每个字的速度本身在 ±25% 之间浮动
        let per_char_ms = 1000.0 / self.effective_speed();
        let mut typing_ms = 0.0;
        for _ in 0..reply_text.chars().count() {
            typing_ms += per_char_ms * (0.75 + fastrand::f32() * 0.5);
        }

        (read_ms as u64 + think_ms + typing_ms as u64).clamp(150, cap)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用的纯节奏器：不读全局配置
    fn timing() -> ResponseTiming {
        ResponseTiming {
            base_typing_speed: 5.0,
            speed_modifier: 1.0,
        }
    }

    const CAP: u64 = 4000;

    #[test]
    fn short_replies_are_quick_reflexes() {
        let t = timing();
        for _ in 0..50 {
            let d = t.calculate_delay("嗯", "在吗", CAP);
            assert!(d <= 450, "短回复该是秒回：{d}");
        }
    }

    #[test]
    fn longer_replies_take_longer_on_average() {
        let t = timing();
        let mean = |text: &str| {
            let n = 200;
            let sum: u64 = (0..n)
                .map(|_| t.calculate_delay(text, "随便说点什么", CAP))
                .sum();
            sum / n
        };
        let short = mean("好呀");
        let long = mean("这个我得想想，你先说说你那边什么情况，别急着下结论");
        assert!(long > short, "长回复平均该更慢：{long} vs {short}");
    }

    #[test]
    fn delays_are_not_quantized() {
        // 旧实现把延迟压在少数几个档位上；现在应当有足够多的不同取值
        let t = timing();
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            seen.insert(t.calculate_delay("这个我得想想再说", "你吃饭了吗", CAP));
        }
        assert!(seen.len() > 80, "延迟取值过于集中：{} 种", seen.len());
    }

    #[test]
    fn reading_long_input_costs_more_but_is_capped() {
        let t = timing();
        let short_in: u64 = (0..200)
            .map(|_| t.calculate_delay("这句我说两句", "嗯", CAP))
            .sum();
        let long_in: u64 = (0..200)
            .map(|_| t.calculate_delay("这句我说两句", &"很长的一段话".repeat(40), CAP))
            .sum();
        assert!(
            long_in > short_in,
            "读更长的话该更慢：{long_in} vs {short_in}"
        );
        // 读的时间有上限：再长也不会无限增长
        let absurd: u64 = (0..200)
            .map(|_| t.calculate_delay("这句我说两句", &"话".repeat(5000), CAP))
            .sum();
        assert!(
            absurd < long_in * 2,
            "阅读时间该有上限：{absurd} vs {long_in}"
        );
    }

    #[test]
    fn delay_never_exceeds_the_configured_cap() {
        let t = timing();
        for _ in 0..100 {
            let d = t.calculate_delay(
                "一段挺长的话，用来试探上限在哪里，再多写几个字",
                "在吗",
                800,
            );
            assert!(d <= 800, "延迟超过上限：{d}");
        }
    }
}
