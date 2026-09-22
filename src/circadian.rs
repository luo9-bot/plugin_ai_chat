//! 昼夜节律系统
//!
//! 将现有 proactive 模块的免打扰时段（二元开关）扩展为
//! 基于正弦曲线的连续精力/思维/社交变化模型。
//!
//! 每个人格有自定义偏移量（夜猫子型 vs 早起型），
//! 这些值不以数值形式注入 prompt，而是转化为自然语言描述。

use crate::config;
use std::f32::consts::PI;

/// 昼夜节律状态（实时计算，不持久化）
#[derive(Debug, Clone)]
pub(crate) struct CircadianRhythm {
    /// 当前精力水平 (0.0-1.0)
    pub energy_level: f32,
    /// 思维清晰度 (0.0-1.0)
    pub cognitive_clarity: f32,
    /// 耐心程度 (0.0-1.0)
    pub patience_level: f32,
    /// 社交意愿 (0.0-1.0)
    pub sociability: f32,
    /// 幽默感活跃度 (0.0-1.0)
    pub humor_sensitivity: f32,
    /// 当前小时 (0-23)
    pub current_hour: f32,
}

impl Default for CircadianRhythm {
    fn default() -> Self {
        Self {
            energy_level: 0.5,
            cognitive_clarity: 0.5,
            patience_level: 0.5,
            sociability: 0.5,
            humor_sensitivity: 0.5,
            current_hour: 12.0,
        }
    }
}

/// 计算当前昼夜节律
///
/// 使用正弦曲线模拟人类昼夜节律：
/// - 精力：早晨上升→下午峰值→晚上下降→深夜最低
/// - 思维清晰度：类似精力但略有偏移（早起后有一段迷糊期）
/// - 耐心：下午最低（午後疲倦），早晨和深夜较高
/// - 社交意愿：下午和傍晚最高
/// - 幽默感：晚上较高
pub(crate) fn calculate() -> CircadianRhythm {
    let cfg = config::get();
    let h = &cfg.humanity;

    // 本地时刻（东八区投影只有 `util` 一处定义）
    let time_of_day = crate::util::secs_of_day_cst(crate::util::now_secs()) as f32 / 3600.0;

    // 正弦曲线基础相位：峰值在14:00（下午2点），谷底在2:00（凌晨2点）
    let base_phase = (time_of_day - 14.0) * PI / 12.0;
    let amplitude = h.circadian_amplitude;
    let phase_offset = h.circadian_phase_offset;

    // 精力：主正弦曲线 + 人格偏移
    let energy_raw = (base_phase + phase_offset).cos();
    let energy_level = ((energy_raw * amplitude + 0.5) as f32).clamp(0.0, 1.0);

    // 思维清晰度：精力偏移30分钟（早起迷糊期）
    let clarity_phase = base_phase + phase_offset + 0.13; // ~30分钟偏移
    let clarity_raw = clarity_phase.cos();
    let cognitive_clarity = ((clarity_raw * amplitude + 0.5) as f32).clamp(0.0, 1.0);

    // 耐心：与精力反相（精力差时反而更有耐心等待）
    let patience_raw = (base_phase + phase_offset + PI * 0.3).cos();
    let patience_level = ((patience_raw * amplitude * 0.6 + 0.5) as f32).clamp(0.0, 1.0);

    // 社交意愿：峰值在18:00（傍晚），谷底在6:00（清晨）
    let social_phase = (time_of_day - 18.0) * PI / 12.0 + phase_offset;
    let sociability_raw = social_phase.cos();
    let sociability = ((sociability_raw * amplitude + 0.5) as f32).clamp(0.0, 1.0);

    // 幽默感：晚上更活跃（20:00峰值）
    let humor_phase = (time_of_day - 20.0) * PI / 12.0 + phase_offset;
    let humor_raw = humor_phase.cos();
    let humor_sensitivity = ((humor_raw * amplitude * 0.8 + 0.5) as f32).clamp(0.0, 1.0);

    CircadianRhythm {
        energy_level,
        cognitive_clarity,
        patience_level,
        sociability,
        humor_sensitivity,
        current_hour: time_of_day,
    }
}

/// 检查当前是否在免打扰时段（结合 proactive 配置）
///
/// 判定规则只有一处实现：[`crate::util::hour_in_window`]。
/// 这里曾经与 `mind::wake::is_night()` 各写一遍同样的 if/else——两份
/// 语义相同的代码，任何一侧改动都会让"她睡了没"在不同模块里给出不同答案。
pub(crate) fn is_quiet_hours() -> bool {
    let cfg = config::get();
    crate::util::hour_in_window(
        crate::util::current_hour_cst(),
        cfg.proactive.quiet_start,
        cfg.proactive.quiet_end,
    )
}

/// 获取节律对回复行为的影响权重（用于变速回复等其他模块）
pub(crate) fn get_energy_multiplier() -> f32 {
    let rhythm = calculate();
    // 综合精力+清晰度
    (rhythm.energy_level * 0.6 + rhythm.cognitive_clarity * 0.4).clamp(0.3, 1.0)
}
