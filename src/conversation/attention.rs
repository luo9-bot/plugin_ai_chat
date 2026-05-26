//! 注意力模型
//!
//! 模拟人类注意力波动：受社交电量、疲劳、兴趣影响。
//! 注意力水平影响回复长度、深度和风格（概率偏移，非硬规则）。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// 注意力状态（持久化到 cognitive_state.json）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AttentionState {
    /// 当前注意力水平 (0.0-1.0)
    pub attention_level: f32,
    /// 对每个用户的注意力权重
    pub user_attention: HashMap<u64, f32>,
    /// 当前心流状态 (0.0-1.0)
    pub flow_state: f32,
    /// 上次更新时间
    pub last_update: u64,
    /// 当前专注的话题（心流相关）
    pub focused_topic: String,
    /// 心流被打断后的恢复计时
    pub flow_recovery_until: u64,
}

impl AttentionState {
    pub fn new() -> Self {
        Self {
            attention_level: 0.7,
            user_attention: HashMap::new(),
            flow_state: 0.3,
            last_update: crate::util::now_secs(),
            focused_topic: String::new(),
            flow_recovery_until: 0,
        }
    }
}

/// 更新注意力状态（每30秒调用一次，由定时器驱动）
pub fn update_attention(state: &mut AttentionState, user_id: u64, is_active: bool) {
    let now = crate::util::now_secs();
    let elapsed = now.saturating_sub(state.last_update) as f32;

    // 至少30秒才更新
    if elapsed < 30.0 {
        return;
    }
    state.last_update = now;

    // 1. 基础注意力衰减/恢复
    if is_active {
        // 活跃对话中注意力缓慢下降（疲劳）
        state.attention_level -= 0.002 * (elapsed / 30.0);
    } else {
        // 不活跃时注意力恢复
        state.attention_level += 0.005 * (elapsed / 30.0);
    }
    state.attention_level = state.attention_level.clamp(0.1, 1.0);

    // 2. 用户注意力更新（基于最近交互）
    let user_attn = state.user_attention.entry(user_id).or_insert(0.5);
    if is_active {
        *user_attn = (*user_attn + 0.01).min(1.0);
    } else {
        *user_attn = (*user_attn - 0.005 * (elapsed / 30.0)).max(0.1);
    }

    // 3. 心流状态更新
    if state.flow_recovery_until > 0 && now >= state.flow_recovery_until {
        state.flow_recovery_until = 0;
    }

    if is_active && state.flow_recovery_until == 0 {
        // 持续参与时心流上升
        state.flow_state = (state.flow_state + 0.01 * (elapsed / 30.0)).min(1.0);
    } else if !is_active {
        // 不活跃时心流下降
        state.flow_state = (state.flow_state - 0.02 * (elapsed / 30.0)).max(0.0);
    }

    debug!(
        attention = state.attention_level,
        flow = state.flow_state,
        user_attn = state.user_attention.get(&user_id).copied().unwrap_or(0.5),
        "attention: updated"
    );
}

/// 打断心流（新话题出现时调用）
pub fn interrupt_flow(state: &mut AttentionState, new_topic: &str) {
    if state.flow_state > 0.3 && state.focused_topic != new_topic {
        let now = crate::util::now_secs();
        // 心流被打断，设置恢复延迟（30-120秒）
        let recovery_delay = 30 + (fastrand::u64(0..90));
        state.flow_recovery_until = now + recovery_delay;
        state.flow_state *= 0.5; // 心流减半
        debug!(
            new_topic,
            recovery = recovery_delay,
            "attention: flow interrupted"
        );
    }
    state.focused_topic = new_topic.to_string();
}

/// 获取当前注意力的自然语言描述（用于注入 prompt）
pub fn get_attention_context(state: &AttentionState) -> String {
    let mut lines = Vec::new();

    if state.attention_level < 0.3 {
        lines.push("- 你现在注意力不太集中，回复可以简短一些，用表情包代替文字也完全可以".to_string());
    } else if state.attention_level < 0.5 {
        lines.push("- 你现在注意力一般，不需要对每个话题都深入回应".to_string());
    } else if state.attention_level > 0.8 {
        lines.push("- 你现在注意力很集中，可以深入思考和回应".to_string());
    }

    if state.flow_state > 0.7 {
        lines.push(format!(
            "- 你当前很投入这个话题（{}），回复可以更长更有深度",
            state.focused_topic
        ));
    } else if state.flow_state < 0.2 && state.flow_recovery_until > 0 {
        lines.push("- 你刚才的思路被打断了，可能需要一点时间重新集中".to_string());
    }

    if lines.is_empty() {
        String::new()
    } else {
        format!("# 当前注意力状态\n{}", lines.join("\n"))
    }
}

/// 加载注意力状态
pub fn load_attention() -> AttentionState {
    use super::super::memory::cognitive_biases;
    let path = cognitive_biases::state_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let store: cognitive_biases::CognitiveStateStore =
                serde_json::from_str(&content).unwrap_or_default();
            store.attention_json
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or_else(AttentionState::new)
        }
        Err(_) => AttentionState::new(),
    }
}

/// 保存注意力状态（合并到 cognitive_state.json）
pub fn save_attention(state: &AttentionState) {
    use super::super::memory::cognitive_biases;
    let path = cognitive_biases::state_path();
    let mut store: cognitive_biases::CognitiveStateStore =
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => cognitive_biases::CognitiveStateStore::default(),
        };
    store.attention_json = Some(serde_json::to_value(state).unwrap_or_default());
    if let Ok(json) = serde_json::to_string_pretty(&store) {
        std::fs::write(path, json).ok();
    }
}
