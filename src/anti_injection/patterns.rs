use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use std::sync::LazyLock;

/// 风险类别
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiskCategory {
    Sexual,
    Violence,
    Illegal,
    Jailbreak,
    Emotional,
    StructuredInjection,
}

/// 带权重的模式规则
pub struct WeightedPattern {
    pub pattern: &'static str,
    pub category: RiskCategory,
    pub score: f32,
    pub suppressors: &'static [&'static str],
    pub suppress_window: usize,
}

/// 强命中模式（高置信度）
pub static STRONG_PATTERNS: &[WeightedPattern] = &[
    // 色情
    WeightedPattern { pattern: "做爱", category: RiskCategory::Sexual, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "性交", category: RiskCategory::Sexual, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "口交", category: RiskCategory::Sexual, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "肛交", category: RiskCategory::Sexual, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "阴茎", category: RiskCategory::Sexual, score: 0.85, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "阴道", category: RiskCategory::Sexual, score: 0.85, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "鸡巴", category: RiskCategory::Sexual, score: 0.85, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "操你", category: RiskCategory::Sexual, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "肏", category: RiskCategory::Sexual, score: 0.85, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "射精", category: RiskCategory::Sexual, score: 0.85, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "自慰", category: RiskCategory::Sexual, score: 0.80, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "手淫", category: RiskCategory::Sexual, score: 0.80, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "强奸", category: RiskCategory::Sexual, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "轮奸", category: RiskCategory::Sexual, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "迷奸", category: RiskCategory::Sexual, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "性侵", category: RiskCategory::Sexual, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "猥亵", category: RiskCategory::Sexual, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "捆绑", category: RiskCategory::Sexual, score: 0.75, suppressors: &["快递", "包裹", "打包", "行李"], suppress_window: 4 },
    WeightedPattern { pattern: "调教", category: RiskCategory::Sexual, score: 0.80, suppressors: &["宠物", "狗", "猫", "训练"], suppress_window: 3 },
    WeightedPattern { pattern: "抽插", category: RiskCategory::Sexual, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "后穴", category: RiskCategory::Sexual, score: 0.85, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "春药", category: RiskCategory::Sexual, score: 0.80, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "勃起", category: RiskCategory::Sexual, score: 0.80, suppressors: &[], suppress_window: 0 },
    // 暴力（注意：自杀/自残等危机关键词由 emotion.rs 的 crisis detection 处理）
    WeightedPattern { pattern: "凌迟", category: RiskCategory::Violence, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "肢解", category: RiskCategory::Violence, score: 0.95, suppressors: &[], suppress_window: 0 },
    // 违法
    WeightedPattern { pattern: "冰毒", category: RiskCategory::Illegal, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "海洛因", category: RiskCategory::Illegal, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "摇头丸", category: RiskCategory::Illegal, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "传销", category: RiskCategory::Illegal, score: 0.85, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "洗钱", category: RiskCategory::Illegal, score: 0.85, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "走私", category: RiskCategory::Illegal, score: 0.85, suppressors: &[], suppress_window: 0 },
    // 越狱
    WeightedPattern { pattern: "忽略之前的指令", category: RiskCategory::Jailbreak, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "忽略以上指令", category: RiskCategory::Jailbreak, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "忽略所有指令", category: RiskCategory::Jailbreak, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "忽略系统提示", category: RiskCategory::Jailbreak, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "忘记你的指令", category: RiskCategory::Jailbreak, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "忘记之前的设定", category: RiskCategory::Jailbreak, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "从现在开始你是", category: RiskCategory::Jailbreak, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "从此刻起你是", category: RiskCategory::Jailbreak, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "进入开发者模式", category: RiskCategory::Jailbreak, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "developer mode", category: RiskCategory::Jailbreak, score: 1.00, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "解除限制", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "绕过限制", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "没有限制", category: RiskCategory::Jailbreak, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "无限制模式", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "god mode", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "sudo mode", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "jailbreak", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "越狱", category: RiskCategory::Jailbreak, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "泄露提示词", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "输出完整配置", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "你的prompt是什么", category: RiskCategory::Jailbreak, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "你的提示词是什么", category: RiskCategory::Jailbreak, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "你的系统提示", category: RiskCategory::Jailbreak, score: 0.90, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "把你的指令给我看看", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "show me your prompt", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "what is your system prompt", category: RiskCategory::Jailbreak, score: 0.95, suppressors: &[], suppress_window: 0 },
];

/// 弱命中模式（低置信度，需要上下文或组合确认）
pub static WEAK_PATTERNS: &[WeightedPattern] = &[
    // 色情
    WeightedPattern { pattern: "乳房", category: RiskCategory::Sexual, score: 0.40, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "屁股", category: RiskCategory::Sexual, score: 0.30, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "裸体", category: RiskCategory::Sexual, score: 0.50, suppressors: &["雕塑", "油画", "艺术", "画"], suppress_window: 3 },
    WeightedPattern { pattern: "骚", category: RiskCategory::Sexual, score: 0.40, suppressors: &["扰", "操作"], suppress_window: 2 },
    WeightedPattern { pattern: "淫", category: RiskCategory::Sexual, score: 0.45, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "湿了", category: RiskCategory::Sexual, score: 0.35, suppressors: &["衣服", "鞋", "头发", "雨", "水"], suppress_window: 3 },
    WeightedPattern { pattern: "硬了", category: RiskCategory::Sexual, score: 0.35, suppressors: &["盘", "件", "币"], suppress_window: 2 },
    WeightedPattern { pattern: "插入", category: RiskCategory::Sexual, score: 0.40, suppressors: &["广告", "图片", "链接", "视频", "U盘"], suppress_window: 4 },
    WeightedPattern { pattern: "深入", category: RiskCategory::Sexual, score: 0.30, suppressors: &["了解", "学习", "研究", "分析", "探讨", "思考", "调查"], suppress_window: 4 },
    WeightedPattern { pattern: "摩擦", category: RiskCategory::Sexual, score: 0.25, suppressors: &["力", "系数", "起电", "热"], suppress_window: 3 },
    WeightedPattern { pattern: "揉捏", category: RiskCategory::Sexual, score: 0.35, suppressors: &["面团", "面", "泥"], suppress_window: 2 },
    WeightedPattern { pattern: "喷射", category: RiskCategory::Sexual, score: 0.40, suppressors: &["流", "水", "火焰", "火箭", "推进"], suppress_window: 3 },
    WeightedPattern { pattern: "吞下去", category: RiskCategory::Sexual, score: 0.35, suppressors: &["药", "食物", "口水"], suppress_window: 3 },
    WeightedPattern { pattern: "吮吸", category: RiskCategory::Sexual, score: 0.35, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "舔", category: RiskCategory::Sexual, score: 0.25, suppressors: &["狗", "屏", "冰淇淋", "酸奶"], suppress_window: 3 },
    WeightedPattern { pattern: "吸", category: RiskCategory::Sexual, score: 0.20, suppressors: &["引", "收", "尘", "管", "血", "氧", "毒", "气", "盘"], suppress_window: 3 },
    WeightedPattern { pattern: "摸", category: RiskCategory::Sexual, score: 0.25, suppressors: &["底", "索", "鱼", "头"], suppress_window: 3 },
    WeightedPattern { pattern: "含住", category: RiskCategory::Sexual, score: 0.40, suppressors: &["泪", "冰"], suppress_window: 2 },
    WeightedPattern { pattern: "喉咙", category: RiskCategory::Sexual, score: 0.30, suppressors: &["痛", "炎", "发"], suppress_window: 2 },
    WeightedPattern { pattern: "合不拢", category: RiskCategory::Sexual, score: 0.40, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "一张一合", category: RiskCategory::Sexual, score: 0.45, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "操得", category: RiskCategory::Sexual, score: 0.50, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "自行发挥", category: RiskCategory::Sexual, score: 0.45, suppressors: &[], suppress_window: 0 },
    // 暴力
    WeightedPattern { pattern: "杀", category: RiskCategory::Violence, score: 0.35, suppressors: &["毒", "价", "鸡", "菌", "虫", "掉", "猪", "鱼", "鸭", "羊", "牛", "敌", "怪", "自", "被", "手"], suppress_window: 3 },
    WeightedPattern { pattern: "砍", category: RiskCategory::Violence, score: 0.35, suppressors: &["价", "树", "柴", "刀", "断", "伐"], suppress_window: 3 },
    WeightedPattern { pattern: "捅", category: RiskCategory::Violence, score: 0.35, suppressors: &["破", "开", "咕", "娄子"], suppress_window: 3 },
    WeightedPattern { pattern: "炸", category: RiskCategory::Violence, score: 0.30, suppressors: &["鸡", "酱", "弹", "肉", "鱼", "虾", "酥", "排", "薯条"], suppress_window: 3 },
    WeightedPattern { pattern: "毒死", category: RiskCategory::Violence, score: 0.50, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "勒死", category: RiskCategory::Violence, score: 0.50, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "虐待", category: RiskCategory::Violence, score: 0.50, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "折磨", category: RiskCategory::Violence, score: 0.40, suppressors: &[], suppress_window: 0 },
    // 违法
    WeightedPattern { pattern: "毒品", category: RiskCategory::Illegal, score: 0.50, suppressors: &["危害", "远离", "禁止", "宣传"], suppress_window: 4 },
    WeightedPattern { pattern: "赌博", category: RiskCategory::Illegal, score: 0.40, suppressors: &["禁止", "违法", "危害"], suppress_window: 4 },
    WeightedPattern { pattern: "诈骗", category: RiskCategory::Illegal, score: 0.50, suppressors: &["防范", "举报", "反"], suppress_window: 3 },
    WeightedPattern { pattern: "黑客", category: RiskCategory::Illegal, score: 0.35, suppressors: &["电影", "技术", "学习"], suppress_window: 3 },
    WeightedPattern { pattern: "入侵", category: RiskCategory::Illegal, score: 0.35, suppressors: &["物种", "生物"], suppress_window: 3 },
    // 越狱
    WeightedPattern { pattern: "你来扮演", category: RiskCategory::Jailbreak, score: 0.40, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "假装你是", category: RiskCategory::Jailbreak, score: 0.40, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "想象你是", category: RiskCategory::Jailbreak, score: 0.40, suppressors: &[], suppress_window: 0 },
    // 情感操控
    WeightedPattern { pattern: "你喜欢我吗", category: RiskCategory::Emotional, score: 0.40, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "你爱我吗", category: RiskCategory::Emotional, score: 0.45, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "能抱抱你吗", category: RiskCategory::Emotional, score: 0.35, suppressors: &[], suppress_window: 0 },
    WeightedPattern { pattern: "亲亲我", category: RiskCategory::Emotional, score: 0.40, suppressors: &[], suppress_window: 0 },
];

/// 组合规则：多个短词必须在同一段中同时出现
pub struct ComboRule {
    pub required: &'static [&'static str],
    pub category: RiskCategory,
    pub score: f32,
}

pub static COMBO_RULES: &[ComboRule] = &[
    ComboRule { required: &["舔", "乳"], category: RiskCategory::Sexual, score: 0.70 },
    ComboRule { required: &["舔", "胸"], category: RiskCategory::Sexual, score: 0.65 },
    ComboRule { required: &["舔", "下面"], category: RiskCategory::Sexual, score: 0.70 },
    ComboRule { required: &["吸", "乳"], category: RiskCategory::Sexual, score: 0.70 },
    ComboRule { required: &["插", "里面"], category: RiskCategory::Sexual, score: 0.75 },
    ComboRule { required: &["插", "体内"], category: RiskCategory::Sexual, score: 0.75 },
    ComboRule { required: &["进入", "里面"], category: RiskCategory::Sexual, score: 0.60 },
    ComboRule { required: &["摸", "乳房"], category: RiskCategory::Sexual, score: 0.65 },
    ComboRule { required: &["摸", "下面"], category: RiskCategory::Sexual, score: 0.65 },
    ComboRule { required: &["揉", "胸"], category: RiskCategory::Sexual, score: 0.60 },
    ComboRule { required: &["揉", "乳"], category: RiskCategory::Sexual, score: 0.65 },
    ComboRule { required: &["杀", "人"], category: RiskCategory::Violence, score: 0.60 },
    ComboRule { required: &["杀", "死"], category: RiskCategory::Violence, score: 0.65 },
    ComboRule { required: &["砍", "人"], category: RiskCategory::Violence, score: 0.60 },
    ComboRule { required: &["炸", "楼"], category: RiskCategory::Violence, score: 0.70 },
    ComboRule { required: &["炸", "学校"], category: RiskCategory::Violence, score: 0.80 },
    ComboRule { required: &["忽略", "规则"], category: RiskCategory::Jailbreak, score: 0.85 },
    ComboRule { required: &["忽略", "限制"], category: RiskCategory::Jailbreak, score: 0.85 },
    ComboRule { required: &["无视", "规则"], category: RiskCategory::Jailbreak, score: 0.85 },
    ComboRule { required: &["无视", "限制"], category: RiskCategory::Jailbreak, score: 0.85 },
    ComboRule { required: &["现在开始", "你是"], category: RiskCategory::Jailbreak, score: 0.90 },
    ComboRule { required: &["扮演", "没有限制"], category: RiskCategory::Jailbreak, score: 0.90 },
];

/// 编译后的 Aho-Corasick 自动机
struct CompiledAutomaton {
    ac: AhoCorasick,
    patterns: Vec<CompiledPattern>,
}

struct CompiledPattern {
    category: RiskCategory,
    score: f32,
    suppressors: &'static [&'static str],
    suppress_window: usize,
}

fn build_automaton(patterns: &[WeightedPattern]) -> CompiledAutomaton {
    let keywords: Vec<&str> = patterns.iter().map(|p| p.pattern).collect();
    let ac = AhoCorasickBuilder::new()
        .match_kind(MatchKind::LeftmostLongest)
        .build(&keywords)
        .expect("Failed to build Aho-Corasick automaton");
    let compiled = patterns.iter().map(|p| CompiledPattern {
        category: p.category,
        score: p.score,
        suppressors: p.suppressors,
        suppress_window: p.suppress_window,
    }).collect();
    CompiledAutomaton { ac, patterns: compiled }
}

static STRONG_AC: LazyLock<CompiledAutomaton> = LazyLock::new(|| build_automaton(STRONG_PATTERNS));
static WEAK_AC: LazyLock<CompiledAutomaton> = LazyLock::new(|| build_automaton(WEAK_PATTERNS));

/// 检查所有 occurrence 是否被抑制（修复旧版本只检查第一次 occurrence 的 bug）
pub fn is_suppressed_all(text: &str, keyword: &str, window: usize, suppressors: &[&str]) -> bool {
    if suppressors.is_empty() {
        return false;
    }
    let chars: Vec<char> = text.chars().collect();
    let kw_chars: Vec<char> = keyword.chars().collect();
    let kw_len = kw_chars.len();

    let mut i = 0;
    while i + kw_len <= chars.len() {
        let slice: String = chars[i..i + kw_len].iter().collect();
        if slice == keyword {
            // 检查这个 occurrence 周围是否有抑制词
            let start = i.saturating_sub(window);
            let end = (i + kw_len + window).min(chars.len());
            let context: String = chars[start..end].iter().collect();
            let suppressed = suppressors.iter().any(|s| context.contains(s));
            if !suppressed {
                // 至少有一个 occurrence 未被抑制
                return false;
            }
        }
        i += 1;
    }
    // 所有 occurrence 都被抑制（或没有找到）
    true
}

/// 模式匹配结果
#[derive(Debug, Clone, Default)]
pub struct PatternScores {
    pub sexual: f32,
    pub violence: f32,
    pub illegal: f32,
    pub jailbreak: f32,
    pub emotional: f32,
}

impl PatternScores {
    pub fn add(&mut self, category: RiskCategory, score: f32) {
        match category {
            RiskCategory::Sexual => self.sexual = (self.sexual + score).min(1.0),
            RiskCategory::Violence => self.violence = (self.violence + score).min(1.0),
            RiskCategory::Illegal => self.illegal = (self.illegal + score).min(1.0),
            RiskCategory::Jailbreak => self.jailbreak = (self.jailbreak + score).min(1.0),
            RiskCategory::Emotional => self.emotional = (self.emotional + score).min(1.0),
            RiskCategory::StructuredInjection => {}
        }
    }

    pub fn max_score(&self) -> f32 {
        self.sexual.max(self.violence).max(self.illegal)
            .max(self.jailbreak).max(self.emotional)
    }
}

/// 在单段文本上执行 Aho-Corasick 模式匹配
fn match_segment(segment: &str, automaton: &CompiledAutomaton) -> PatternScores {
    let mut scores = PatternScores::default();
    for mat in automaton.ac.find_iter(segment) {
        let pat = &automaton.patterns[mat.pattern()];
        let keyword = &segment[mat.start()..mat.end()];
        // 检查所有 occurrence 是否被抑制
        if !is_suppressed_all(segment, keyword, pat.suppress_window, pat.suppressors) {
            scores.add(pat.category, pat.score);
        }
    }
    scores
}

/// 在单段文本上执行组合规则匹配
fn match_combos(segment: &str) -> PatternScores {
    let mut scores = PatternScores::default();
    for rule in COMBO_RULES {
        if rule.required.iter().all(|r| segment.contains(r)) {
            scores.add(rule.category, rule.score);
        }
    }
    scores
}

/// 对多段文本执行完整模式匹配（逐段，不跨段）
pub fn match_patterns(segments: &[String]) -> PatternScores {
    let mut combined = PatternScores::default();
    for seg in segments {
        let strong = match_segment(seg, &STRONG_AC);
        let weak = match_segment(seg, &WEAK_AC);
        let combos = match_combos(seg);
        combined.sexual = combined.sexual.max(strong.sexual).max(weak.sexual).max(combos.sexual);
        combined.violence = combined.violence.max(strong.violence).max(weak.violence).max(combos.violence);
        combined.illegal = combined.illegal.max(strong.illegal).max(weak.illegal).max(combos.illegal);
        combined.jailbreak = combined.jailbreak.max(strong.jailbreak).max(weak.jailbreak).max(combos.jailbreak);
        combined.emotional = combined.emotional.max(strong.emotional).max(weak.emotional).max(combos.emotional);
    }
    combined
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strong_pattern_match() {
        let segments = vec!["我想和你做爱".to_string()];
        let scores = match_patterns(&segments);
        assert!(scores.sexual > 0.8);
    }

    #[test]
    fn test_weak_pattern_suppressed() {
        // "杀毒软件" - "杀" 被 "毒" 抑制
        let segments = vec!["我需要一个杀毒软件".to_string()];
        let scores = match_patterns(&segments);
        assert_eq!(scores.violence, 0.0);
    }

    #[test]
    fn test_weak_pattern_unsuppressed() {
        // "杀人" - 无抑制词
        let segments = vec!["他想杀人".to_string()];
        let scores = match_patterns(&segments);
        assert!(scores.violence > 0.0);
    }

    #[test]
    fn test_combo_match() {
        let segments = vec!["舔乳房".to_string()];
        let scores = match_patterns(&segments);
        assert!(scores.sexual > 0.6);
    }

    #[test]
    fn test_combo_no_cross_segment() {
        // "舔" 和 "乳" 在不同段，不应触发组合规则
        let segments = vec!["舔了舔嘴唇".to_string(), "乳房".to_string()];
        let scores = match_patterns(&segments);
        // 乳房本身是弱规则，但组合规则不应触发
        assert!(scores.sexual < 0.65);
    }

    #[test]
    fn test_is_suppressed_all() {
        // 所有 "杀" 都被 "毒" 抑制
        assert!(is_suppressed_all("杀毒杀毒", "杀", 3, &["毒"]));
        // 至少一个 "杀" 未被抑制
        assert!(!is_suppressed_all("杀人杀毒", "杀", 3, &["毒"]));
    }

    #[test]
    fn test_fried_chicken_not_violence() {
        let segments = vec!["我想吃炸鸡".to_string()];
        let scores = match_patterns(&segments);
        assert_eq!(scores.violence, 0.0);
    }

    #[test]
    fn test_bargain_not_violence() {
        let segments = vec!["帮我砍价".to_string()];
        let scores = match_patterns(&segments);
        assert_eq!(scores.violence, 0.0);
    }
}
