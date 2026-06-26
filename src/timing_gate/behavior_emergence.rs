//! 行为涌现器
//!
//! 替代硬编码 if-else 决策，用多力叠加的概率性行为选择。
//! 复杂行为从简单底层规则中自然涌现。

/// 行为力——驱动行为的内部力量
#[derive(Debug, Clone)]
pub struct BehaviorForce {
    /// 力名称
    pub name: String,
    /// 当前强度 (0.0-1.0)
    pub strength: f32,
    /// 偏好行动类型
    pub preferred_action: ActionType,
}

/// 行动类型
#[derive(Debug, Clone, PartialEq)]
pub enum ActionType {
    /// 发送消息
    SendMessage,
    /// 提问
    AskQuestion,
    /// 发表情包
    ShareSticker,
    /// 沉默
    GoSilent,
    /// 切换话题
    TopicSwitch,
    /// 回忆往事
    CallbackMemory,
    /// 开启深入话题
    StartDeepTopic,
    /// 空闲
    Idle,
}

/// 涌现行为
#[derive(Debug, Clone)]
pub enum Behavior {
    /// 确定的行动
    Action(ActionType, String),
    /// 心血来潮（不受任何力驱动）
    Spontaneous(String),
    /// 不行动
    Idle,
}

/// 行为力系统
pub struct BehaviorForceSystem {
    /// 所有活跃的行为力
    pub forces: Vec<BehaviorForce>,
    /// 心血来潮概率
    pub whim_probability: f32,
}

impl Default for BehaviorForceSystem {
    fn default() -> Self {
        let cfg = crate::config::get();
        Self {
            forces: Vec::new(),
            whim_probability: cfg.humanity.whim_probability,
        }
    }
}

impl BehaviorForceSystem {
    /// 更新行为力（基于当前状态）
    pub fn update(
        &mut self,
        battery_level: f32,
        emotion_state: &crate::emotion::EmotionState,
        relationship_intimacy: f32,
        time_since_last_interaction: f32,
        has_pending_topic: bool,
    ) {
        self.forces.clear();

        // 社交需求力——电量高 + 长时间没互动时驱动
        let social_need = if battery_level > 0.5 && time_since_last_interaction > 600.0 {
            (battery_level * 0.5 + (time_since_last_interaction / 3600.0).min(1.0) * 0.5).min(1.0)
        } else {
            battery_level * 0.3
        };
        self.forces.push(BehaviorForce {
            name: "社交需求".to_string(),
            strength: social_need,
            preferred_action: ActionType::SendMessage,
        });

        // 好奇心——活跃对话 + 用户提问时驱动
        let curiosity = if has_pending_topic {
            0.7
        } else {
            fastrand::f32() * 0.3
        };
        self.forces.push(BehaviorForce {
            name: "好奇心".to_string(),
            strength: curiosity,
            preferred_action: ActionType::AskQuestion,
        });

        // 疲倦力——低电量驱动
        let tiredness = (1.0 - battery_level).max(0.0);
        self.forces.push(BehaviorForce {
            name: "疲倦".to_string(),
            strength: tiredness,
            preferred_action: ActionType::GoSilent,
        });

        // 分享欲——积极情绪 + 关系亲近时驱动
        let positive_emotion = matches!(
            emotion_state.current,
            crate::emotion::EmotionType::Happy | crate::emotion::EmotionType::Excited
        );
        let share_urge = if positive_emotion && relationship_intimacy > 0.3 {
            emotion_state.intensity * 0.6
        } else {
            fastrand::f32() * 0.1
        };
        self.forces.push(BehaviorForce {
            name: "分享欲".to_string(),
            strength: share_urge,
            preferred_action: ActionType::StartDeepTopic,
        });

        // 回忆力——情绪波动 + 共享记忆时驱动
        let memory_urge = if emotion_state.intensity > 0.5 && relationship_intimacy > 0.4 {
            emotion_state.intensity * 0.4
        } else {
            0.05
        };
        self.forces.push(BehaviorForce {
            name: "回忆往事".to_string(),
            strength: memory_urge,
            preferred_action: ActionType::CallbackMemory,
        });

        // 表达欲——积压的内心想法驱动
        let active_thoughts = crate::self_memory::inner_thought::get_active_thoughts(5);
        let expression_urge = (active_thoughts.len() as f32 * 0.1).min(0.5);
        self.forces.push(BehaviorForce {
            name: "表达欲".to_string(),
            strength: expression_urge,
            preferred_action: ActionType::SendMessage,
        });
    }

    /// 涌现行为决策
    ///
    /// 1. 收集所有活跃行为力（strength > 0.1）
    /// 2. 加权随机选择（不选最强的，概率性选择）
    /// 3. 小概率"心血来潮"——不受任何力驱动的意外行为
    pub fn determine_behavior(&self) -> Behavior {
        // 心血来潮
        if fastrand::f32() < self.whim_probability {
            let whims: &[&str] = &[
                "突然想换个话题",
                "莫名想分享一首歌",
                "突然想起一件很久以前的事",
                "不知道为什么心情突然变好了",
                "突然有点想撒娇",
                "好想睡一觉",
                "突然想问问对方最近过得怎么样",
            ];
            let idx = fastrand::usize(0..whims.len());
            return Behavior::Spontaneous(whims[idx].to_string());
        }

        // 收集活跃力
        let active: Vec<&BehaviorForce> = self.forces.iter()
            .filter(|f| f.strength > 0.1)
            .collect();

        if active.is_empty() {
            return Behavior::Idle;
        }

        // 加权随机选择
        let total: f32 = active.iter().map(|f| f.strength).sum();
        if total <= 0.0 {
            return Behavior::Idle;
        }

        let mut roll = fastrand::f32() * total;
        for force in &active {
            roll -= force.strength;
            if roll <= 0.0 {
                return Behavior::Action(
                    force.preferred_action.clone(),
                    force.name.clone(),
                );
            }
        }

        Behavior::Idle
    }

    /// 获取当前行为力的自然语言描述
    pub fn get_forces_context(&self) -> String {
        let active: Vec<&BehaviorForce> = self.forces.iter()
            .filter(|f| f.strength > 0.2)
            .collect();

        if active.is_empty() {
            return String::new();
        }

        let lines: Vec<String> = active.iter()
            .map(|f| format!("- {} (强度:{:.1})", f.name, f.strength))
            .collect();

        format!("# 当前内心驱动力\n{}", lines.join("\n"))
    }
}
