use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::time::SystemTime;
use tracing::{debug, info};

/// AI 情绪分析提示词
const ANALYZE_PROMPT: &str = r#"分析以下对话中用户的情绪状态。根据用户的消息内容、语气和上下文判断情绪。

返回 JSON 格式（不要输出其他内容）:
{"emotion":"情绪类型","intensity":0.0~1.0}

情绪类型必须是以下之一:
- neutral (平静)
- happy (开心)
- sad (难过)
- thinking (思考)
- surprised (惊讶)
- angry (生气)
- shy (害羞)
- worried (担忧)
- tired (疲惫)
- excited (兴奋)

intensity 为 0.0~1.0 的浮点数，表示情绪强度。

示例:
用户说 "太好了！终于考过了！" → {"emotion":"excited","intensity":0.8}
用户说 "嗯，知道了" → {"emotion":"neutral","intensity":0.2}
用户说 "好累啊不想动" → {"emotion":"tired","intensity":0.6}"#;

/// 危机等级：用于检测用户是否处于自残/自杀等极端情境
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, PartialOrd)]
pub enum CrisisLevel {
    None,
    Mild,   // 情绪低落、消极，需要关注
    Severe, // 明确的自残/自杀信号，需要立即干预
}

impl Default for CrisisLevel {
    fn default() -> Self {
        CrisisLevel::None
    }
}

impl CrisisLevel {
    pub fn is_crisis(&self) -> bool {
        *self >= CrisisLevel::Mild
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum EmotionType {
    Neutral,
    Happy,
    Sad,
    Thinking,
    Surprised,
    Angry,
    Shy,
    Worried,
    Tired,
    Excited,
}

impl EmotionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::Happy => "happy",
            Self::Sad => "sad",
            Self::Thinking => "thinking",
            Self::Surprised => "surprised",
            Self::Angry => "angry",
            Self::Shy => "shy",
            Self::Worried => "worried",
            Self::Tired => "tired",
            Self::Excited => "excited",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "happy" | "开心" | "高兴" => Self::Happy,
            "sad" | "难过" | "伤心" => Self::Sad,
            "thinking" | "思考" | "沉思" => Self::Thinking,
            "surprised" | "惊讶" | "吃惊" => Self::Surprised,
            "angry" | "生气" | "愤怒" => Self::Angry,
            "shy" | "害羞" | "羞涩" => Self::Shy,
            "worried" | "担心" | "担忧" => Self::Worried,
            "tired" | "疲惫" | "困倦" => Self::Tired,
            "excited" | "兴奋" | "激动" => Self::Excited,
            _ => Self::Neutral,
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Neutral => "平静",
            Self::Happy => "开心",
            Self::Sad => "难过",
            Self::Thinking => "沉思",
            Self::Surprised => "惊讶",
            Self::Angry => "有些不悦",
            Self::Shy => "害羞",
            Self::Worried => "担忧",
            Self::Tired => "疲惫",
            Self::Excited => "兴奋",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmotionState {
    pub current: EmotionType,
    pub intensity: f32,
    pub last_update: u64,
    pub last_interaction: u64,
    pub interaction_rate: f32,
    pub history: Vec<(EmotionType, u64)>,
    /// 最近一次检测到的危机等级
    #[serde(default)]
    pub crisis_level: CrisisLevel,
    /// 上次危机干预时间（用于避免短时间内重复干预）
    #[serde(default)]
    pub last_crisis_intervention: u64,
}

impl Default for EmotionState {
    fn default() -> Self {
        let now = now_secs();
        Self {
            current: EmotionType::Neutral,
            intensity: 0.3,
            last_update: now,
            last_interaction: now,
            interaction_rate: 0.0,
            history: Vec::new(),
            crisis_level: CrisisLevel::None,
            last_crisis_intervention: 0,
        }
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn emotion_path() -> std::path::PathBuf {
    crate::config::data_dir().join("emotion.json")
}

fn load_states() -> HashMap<String, EmotionState> {
    let path = emotion_path();
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
        Err(_) => HashMap::new(),
    }
}

fn save_states(states: &HashMap<String, EmotionState>) {
    let path = emotion_path();
    if let Ok(json) = serde_json::to_string_pretty(states) {
        fs::write(path, json).ok();
    }
}

/// 初始化时调用：返回有情绪状态的用户数量
pub fn user_count() -> usize {
    let states = load_states();
    states.len()
}

pub fn get_state(user_id: u64) -> EmotionState {
    let states = load_states();
    states.get(&user_id.to_string()).cloned().unwrap_or_default()
}

pub fn update_state(user_id: u64, state: EmotionState) {
    let mut states = load_states();
    states.insert(user_id.to_string(), state);
    save_states(&states);
}

pub fn decay(user_id: u64) {
    let cfg = &crate::config::get().emotion;
    let mut state = get_state(user_id);
    let now = now_secs();
    let elapsed = now.saturating_sub(state.last_update) as f32;
    if elapsed < cfg.decay_delay_secs as f32 {
        return;
    }
    let decay = cfg.decay_rate * (elapsed / 3600.0);
    state.intensity = (state.intensity - decay).max(0.0);
    if state.intensity < cfg.neutral_threshold && state.current != EmotionType::Neutral {
        state.current = EmotionType::Neutral;
        state.intensity = 0.3;
    }
    state.last_update = now;
    update_state(user_id, state);
}

pub fn analyze_user_message(user_id: u64, message: &str) -> bool {
    info!(user_id, message = %message.chars().take(30).collect::<String>(), "emotion: 分析用户消息");
    let mut state = get_state(user_id);
    let now = now_secs();

    let time_since_last = now.saturating_sub(state.last_interaction) as f32;
    state.last_interaction = now;
    if time_since_last > 0.0 && time_since_last < 3600.0 {
        let rate = 3600.0 / time_since_last;
        state.interaction_rate = state.interaction_rate * 0.7 + rate * 0.3;
    }

    let (detected, delta) = detect_emotion(message);
    if delta > 0.1 {
        if detected != state.current {
            state.history.push((state.current, now));
            if state.history.len() > 5 {
                state.history.remove(0);
            }
        }
        state.current = detected;
        state.intensity = (state.intensity + delta).min(1.0);
    }

    if state.interaction_rate > crate::config::get().emotion.affinity_threshold {
        state.intensity = (state.intensity + 0.02).min(1.0);
        if state.current == EmotionType::Neutral {
            state.current = EmotionType::Happy;
        }
    }

    state.last_update = now;
    update_state(user_id, state);

    // 危机信号检测
    let crisis = detect_crisis(message);
    update_crisis(user_id, crisis)
}

/// AI 驱动的情绪分析 (发送回复后调用，用于更精准的情绪状态更新)
pub fn ai_analyze(user_id: u64, user_message: &str, ai_reply: &str) {
    let content = format!("用户消息: {}\nAI回复: {}", user_message, ai_reply);

    let result = crate::ai::analyze(ANALYZE_PROMPT, &content);
    match result {
        Ok(raw) => {
            // 尝试从回复中提取 JSON
            let json_str = if let Some(start) = raw.find('{') {
                if let Some(end) = raw[start..].find('}') {
                    &raw[start..start + end + 1]
                } else {
                    &raw
                }
            } else {
                &raw
            };

            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
                let emotion_str = parsed.get("emotion")
                    .and_then(|v| v.as_str())
                    .unwrap_or("neutral");
                let intensity = parsed.get("intensity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.3) as f32;

                let detected = EmotionType::from_str(emotion_str);
                let mut state = get_state(user_id);
                let now = now_secs();

                if detected != state.current {
                    state.history.push((state.current, now));
                    if state.history.len() > 5 {
                        state.history.remove(0);
                    }
                }
                state.current = detected;
                state.intensity = intensity.clamp(0.0, 1.0);
                state.last_update = now;
                update_state(user_id, state);
            }
        }
        Err(e) => {
            debug!(error = %e, "emotion AI analysis failed, falling back to keyword");
            // fallback: 关键词分析
            let (detected, delta) = detect_emotion(user_message);
            if delta > 0.1 {
                let mut state = get_state(user_id);
                let now = now_secs();
                if detected != state.current {
                    state.history.push((state.current, now));
                    if state.history.len() > 5 {
                        state.history.remove(0);
                    }
                }
                state.current = detected;
                state.intensity = (state.intensity + delta).min(1.0);
                state.last_update = now;
                update_state(user_id, state);
            }
        }
    }
}

/// 从后处理分析结果更新情绪状态
pub fn update_from_analysis(user_id: u64, emotion_str: &str, intensity: f32) {
    let detected = EmotionType::from_str(emotion_str);
    let mut state = get_state(user_id);
    let now = now_secs();

    if detected != state.current {
        debug!(user_id, from = ?state.current, to = ?detected, intensity, "emotion: state changed");
        state.history.push((state.current, now));
        if state.history.len() > 5 {
            state.history.remove(0);
        }
    }
    state.current = detected;
    state.intensity = intensity.clamp(0.0, 1.0);
    state.last_update = now;
    update_state(user_id, state);
}

pub fn parse_from_reply(user_id: u64, reply: &str) -> String {
    let mut state = get_state(user_id);
    let now = now_secs();
    let mut cleaned = reply.to_string();

    let markers = ["[emotion:", "[Emotion:", "[EMOTION:"];
    for marker in &markers {
        if let Some(start) = cleaned.find(marker) {
            let tag_start = start + marker.len();
            if let Some(end) = cleaned[tag_start..].find(']') {
                let emotion_str = cleaned[tag_start..tag_start + end].trim();
                let detected = EmotionType::from_str(emotion_str);
                state.history.push((state.current, now));
                if state.history.len() > 5 {
                    state.history.remove(0);
                }
                state.current = detected;
                state.intensity = (state.intensity + 0.2).min(1.0);
                state.last_update = now;
                update_state(user_id, state);

                let full_end = tag_start + end + 1;
                let mut remove_start = start;
                if remove_start > 0 && cleaned.as_bytes().get(remove_start - 1) == Some(&b' ') {
                    remove_start -= 1;
                }
                cleaned.replace_range(remove_start..full_end, "");
                break;
            }
        }
    }
    cleaned
}

fn detect_emotion(message: &str) -> (EmotionType, f32) {
    let pairs: &[(&[&str], EmotionType, f32)] = &[
        (&["哈哈", "嘻嘻", "开心", "高兴", "太好了", "棒", "赞", "爱", "喜欢", "嘿嘿", "哇", "感动", "幸福", "谢谢", "感谢"], EmotionType::Happy, 0.3),
        (&["兴奋", "激动", "太棒了", "爽", "绝了", "666", "厉害"], EmotionType::Excited, 0.4),
        (&["难过", "伤心", "哭", "呜呜", "唉", "可惜", "遗憾", "失望", "孤独", "寂寞"], EmotionType::Sad, 0.3),
        (&["生气", "愤怒", "烦", "讨厌", "气死", "恼火", "受够了", "滚"], EmotionType::Angry, 0.4),
        (&["担心", "焦虑", "紧张", "害怕", "恐惧", "不安", "慌"], EmotionType::Worried, 0.3),
        (&["嗯", "哦", "这样", "好吧", "知道了", "了解"], EmotionType::Neutral, 0.1),
        (&["想", "思考", "为什么", "怎么", "如何", "吗", "呢", "？"], EmotionType::Thinking, 0.15),
        (&["惊", "啊", "天", "不会吧", "真的吗", "居然", "竟然", "没想到"], EmotionType::Surprised, 0.25),
        (&["害羞", "脸红", "不好意思", "讨厌啦", "才不是", "哼"], EmotionType::Shy, 0.3),
        (&["累", "困", "疲", "想睡", "没精神", "懒", "不想动"], EmotionType::Tired, 0.25),
    ];

    let mut best_emotion = EmotionType::Neutral;
    let mut best_delta = 0.0f32;

    for (keywords, emotion, delta) in pairs {
        for kw in *keywords {
            if message.contains(kw) {
                if *delta > best_delta {
                    best_delta = *delta;
                    best_emotion = *emotion;
                }
            }
        }
    }

    let exclaim_count = message.matches('!').count() + message.matches('！').count();
    if exclaim_count >= 2 && best_delta < 0.2 {
        best_delta = 0.2;
        best_emotion = EmotionType::Excited;
    }

    (best_emotion, best_delta)
}

/// 危机信号关键词检测
///
/// 返回检测到的危机等级。Severe 需要立即干预，Mild 需要关注。
pub fn detect_crisis(message: &str) -> CrisisLevel {
    // 严重危机：明确的自残/自杀意图
    let severe_keywords: &[&str] = &[
        "自杀", "自残", "想死", "不想活", "活不下去", "去死", "死掉算了",
        "跳楼", "割腕", "吃药", "上吊", "跳河", "跳海", "遗书",
        "活着没意思", "活着没意义", "不想活了", "活够了", "死了算了",
        "解脱吧", "结束生命", "结束自己", "一了百了", "不如死了",
        "自杀算了", "想去死", "想离开这个世界", "这个世界没什么好留恋",
    ];

    for kw in severe_keywords {
        if message.contains(kw) {
            return CrisisLevel::Severe;
        }
    }

    // 轻度危机：极度消极、绝望情绪
    let mild_keywords: &[&str] = &[
        "不想活", "活着好累", "活着好痛苦", "崩溃了", "撑不下去",
        "没有人在乎", "没有人在意", "没有人爱我", "所有人都讨厌我",
        "我是多余的", "这个世界不需要我", "我很没用", "活着好没意思",
        "好绝望", "绝望了", "看不到希望", "没有希望", "没有未来",
        "没有意义", "一切都没有意义", "什么都不想做", "什么都不重要",
        "好痛苦", "太痛苦了", "受不了了", "真的受不了了",
        "不想面对", "想消失", "想逃", "逃不掉", "被困住了",
    ];

    for kw in mild_keywords {
        if message.contains(kw) {
            return CrisisLevel::Mild;
        }
    }

    CrisisLevel::None
}

/// 更新危机等级并返回是否需要立即干预
///
/// 干预冷却期：同一次危机干预后 30 分钟内不重复触发（但危机等级始终更新）
pub fn update_crisis(user_id: u64, level: CrisisLevel) -> bool {
    if level == CrisisLevel::None {
        return false;
    }

    let mut state = get_state(user_id);
    let now = now_secs();
    state.crisis_level = level;

    // 严重危机立即干预，轻度危机在冷却期外干预
    let should_intervene = match level {
        CrisisLevel::Severe => {
            // 严重危机：每次都需要干预
            state.last_crisis_intervention = now;
            true
        }
        CrisisLevel::Mild => {
            // 轻度危机：30分钟冷却
            if now.saturating_sub(state.last_crisis_intervention) >= 1800 {
                state.last_crisis_intervention = now;
                true
            } else {
                false
            }
        }
        CrisisLevel::None => false,
    };

    update_state(user_id, state);
    should_intervene
}

/// 获取危机干预的指令上下文，注入到 system prompt
pub fn get_crisis_context(crisis: CrisisLevel) -> String {
    match crisis {
        CrisisLevel::None => String::new(),
        CrisisLevel::Mild => CRISIS_MILD_PROMPT.to_string(),
        CrisisLevel::Severe => CRISIS_SEVERE_PROMPT.to_string(),
    }
}

const CRISIS_MILD_PROMPT: &str = r#"# 紧急状态
你注意到对方的情绪非常低落，可能正处于心理危机边缘。请你：
- 先用你角色的语气表达真诚的关心，不要冷冰冰的
- 认真倾听，不要急着给建议或说"振作起来"之类的话
- 让对方知道有人在意他/她
- 在对话的合适位置，自然地提及：如果感觉很难受，可以跟身边信任的人聊聊，或者拨打心理援助热线
- 24小时心理援助热线：全国 400-161-9995 | 北京 010-82951332 | 希望24热线 400-161-9995
- 保持温和耐心，不要说教"#;

const CRISIS_SEVERE_PROMPT: &str = r#"# 极度紧急状态 ⚠️
你察觉到对方正在表达自残或自杀的意图。这是非常严重的信号。请你：
- 第一时间用你角色的语气表达真诚的关心和担忧，让对方感受到有人在乎
- 绝对不要忽视、敷衍、或转移话题
- 不要说"别想太多"、"会好的"这种空洞的话
- 直接但温和地回应："我听到你说的了，我很担心你"
- 让对方知道：这种感受是可以被理解的，但生命很重要
- 务必在回复中包含求助信息：
  · 全国24小时心理援助热线：400-161-9995
  · 北京心理危机研究与干预中心：010-82951332
  · 生命热线：400-821-1215
  · 如果情况紧急，请拨打 110 或 120
- 鼓励对方现在就拨打热线，或者联系身边信任的人
- 你的回复要温暖、真诚、不慌张，像一个真正关心对方的朋友
- 回复可以长一些，分几段发送，不要急着结束对话"#;

pub fn get_prompt_context(user_id: u64) -> String {
    let state = get_state(user_id);
    let mut lines = Vec::new();
    lines.push("# 情绪状态指令".to_string());

    match state.current {
        EmotionType::Neutral => {
            lines.push("- 你当前心情平静，以自然放松的状态对话".into());
        }
        EmotionType::Happy => {
            lines.push("- 你当前心情不错，语气会更轻快积极，可能会多用一些语气词".into());
        }
        EmotionType::Sad => {
            lines.push("- 你当前有些低落，回复可能会更简短，语气稍显沉静".into());
        }
        EmotionType::Thinking => {
            lines.push("- 你当前处于思考状态，回复可能带有沉吟、停顿的感觉".into());
        }
        EmotionType::Surprised => {
            lines.push("- 你当前有些惊讶，可能会用感叹或追问来表达".into());
        }
        EmotionType::Angry => {
            lines.push("- 你当前有些不悦，但不会对用户恶语相向，而是表现为沉默或转移话题".into());
        }
        EmotionType::Shy => {
            lines.push("- 你当前有些害羞，可能会说话吞吞吐吐或用省略号".into());
        }
        EmotionType::Worried => {
            lines.push("- 你当前有些担忧，可能会更关心对方的状况".into());
        }
        EmotionType::Tired => {
            lines.push("- 你当前有些疲惫，回复可能更简短，偶尔带出困倦感".into());
        }
        EmotionType::Excited => {
            lines.push("- 你当前很兴奋，语气会更活泼热情，可能会用更多感叹号".into());
        }
    }

    if state.interaction_rate > 5.0 {
        lines.push("- 你们最近聊得很频繁，关系更亲近了，可以更随意一些".into());
    }

    lines.join("\n")
}

pub fn describe(user_id: u64) -> String {
    let state = get_state(user_id);
    format!("{}({:.1})", state.current.description(), state.intensity)
}
