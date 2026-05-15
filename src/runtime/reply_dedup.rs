/// 回复去重与频率追踪器
///
/// 跟踪最近发送的回复内容和表情包，防止重复和频率失控。
/// 1. ReplyEffectTracker → 追踪回复后用户反馈
/// 2. reply_follow_up_secs → 冷却控制
/// 3. Sticker 去重 → exclude_hashes 参数
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

/// 一条已发送的回复记录
#[derive(Debug, Clone)]
pub struct ReplyRecord {
    /// 回复文本
    pub text: String,
    /// 发送时间
    pub sent_at: Instant,
    /// 目标用户
    pub target_user: u64,
}

/// 一条已发送的表情记录
#[derive(Debug, Clone)]
pub struct StickerRecord {
    pub hash: String,
    pub sent_at: Instant,
}

/// 最近回复缓存
const MAX_RECENT_REPLIES: usize = 16;
/// 最近表情缓存
const MAX_RECENT_STICKERS: usize = 32;
/// 冷却时间：同一用户冷却 (秒)
const USER_COOLDOWN_SECS: u64 = 30;
/// 最短回复间隔 (秒)
const MIN_REPLY_INTERVAL_SECS: u64 = 3;

/// 回复去重状态
pub struct ReplyDedupTracker {
    /// 按群组存最近回复 (group_id -> VecDeque<ReplyRecord>)
    group_replies: HashMap<u64, VecDeque<ReplyRecord>>,
    /// 按群组存最近表情 (group_id -> VecDeque<StickerRecord>)
    group_stickers: HashMap<u64, VecDeque<StickerRecord>>,
    /// 全局回复节奏：上次回复时间
    last_global_reply: Instant,
    /// 已发送文本的完全匹配哈希 (用于跨群去重)
    reply_text_hashes: VecDeque<u64>,
}

impl ReplyDedupTracker {
    pub fn new() -> Self {
        Self {
            group_replies: HashMap::new(),
            group_stickers: HashMap::new(),
            last_global_reply: Instant::now(),
            reply_text_hashes: VecDeque::with_capacity(MAX_RECENT_REPLIES),
        }
    }

    /// 记录一条已发送的回复
    pub fn record_reply(&mut self, group_id: u64, user_id: u64, text: &str) {
        let hash = text_hash(text);
        self.reply_text_hashes.push_back(hash);
        if self.reply_text_hashes.len() > MAX_RECENT_REPLIES {
            self.reply_text_hashes.pop_front();
        }

        let entry = self.group_replies.entry(group_id).or_default();
        entry.push_back(ReplyRecord {
            text: text.to_string(),
            sent_at: Instant::now(),
            target_user: user_id,
        });
        if entry.len() > MAX_RECENT_REPLIES {
            entry.pop_front();
        }
        self.last_global_reply = Instant::now();
    }

    /// 记录一条已发送的表情包
    pub fn record_sticker(&mut self, group_id: u64, hash: &str) {
        let entry = self.group_stickers.entry(group_id).or_default();
        entry.push_back(StickerRecord {
            hash: hash.to_string(),
            sent_at: Instant::now(),
        });
        if entry.len() > MAX_RECENT_STICKERS {
            entry.pop_front();
        }
    }

    /// 检查是否在冷却期（对同一用户的回复间隔）
    pub fn is_user_on_cooldown(&self, group_id: u64, user_id: u64) -> bool {
        if let Some(replies) = self.group_replies.get(&group_id) {
            if let Some(last) = replies.iter().rev().find(|r| r.target_user == user_id) {
                return last.sent_at.elapsed().as_secs() < USER_COOLDOWN_SECS;
            }
        }
        false
    }

    /// 检查最近是否发过非常相似的回复
    pub fn has_recent_similar_reply(&self, group_id: u64, text: &str, threshold: f64) -> bool {
        let hash = text_hash(text);
        // 精确哈希匹配（快速路径）
        if self.reply_text_hashes.iter().any(|&h| h == hash) {
            return true;
        }
        // 群组级别相似度检查（慢路径）
        if let Some(replies) = self.group_replies.get(&group_id) {
            for record in replies.iter().rev().take(8) {
                if text_similarity(text, &record.text) > threshold {
                    return true;
                }
            }
        }
        false
    }

    /// 获取最近的表情包哈希列表（用于排除）
    pub fn get_recent_sticker_hashes(&self, group_id: u64, max_age_secs: u64) -> Vec<String> {
        self.group_stickers
            .get(&group_id)
            .map(|stickers| {
                stickers
                    .iter()
                    .rev()
                    .filter(|s| s.sent_at.elapsed().as_secs() < max_age_secs)
                    .take(16)
                    .map(|s| s.hash.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 检查是否满足最小回复间隔
    pub fn is_min_interval_met(&self) -> bool {
        self.last_global_reply.elapsed().as_secs() >= MIN_REPLY_INTERVAL_SECS
    }
}

/// 简单的文本哈希（用于快速去重）
fn text_hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    // 归一化：去空格、转小写
    let normalized: String = text.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect();
    normalized.hash(&mut hasher);
    hasher.finish()
}

/// 计算两个文本的相似度（基于字符交集 + 完全匹配检测）
fn text_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    // 归一化后比较
    let na: String = a.chars().filter(|c| !c.is_whitespace()).collect();
    let nb: String = b.chars().filter(|c| !c.is_whitespace()).collect();
    if na.len() < 2 || nb.len() < 2 {
        return if na == nb { 1.0 } else { 0.0 };
    }
    // 完全匹配
    if na == nb {
        return 1.0;
    }
    // 字符交集相似度
    let set_a: std::collections::HashSet<char> = na.chars().collect();
    let common = nb.chars().filter(|c| set_a.contains(c)).count();
    common as f64 / na.len().max(nb.len()) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_hash_same() {
        assert_eq!(text_hash("你好吗"), text_hash("你好吗"));
    }

    #[test]
    fn test_text_hash_normalized() {
        assert_eq!(text_hash(" 你好 "), text_hash("你好"));
    }

    #[test]
    fn test_text_similarity_identical() {
        assert!((text_similarity("今天天气真好", "今天天气真好") - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_text_similarity_different() {
        assert!(text_similarity("abc", "xyz") < 0.5);
    }

    #[test]
    fn test_cooldown() {
        let mut tracker = ReplyDedupTracker::new();
        assert!(!tracker.is_user_on_cooldown(1, 100));
        tracker.record_reply(1, 100, "你好");
        assert!(tracker.is_user_on_cooldown(1, 100));
    }

    #[test]
    fn test_recent_similar() {
        let mut tracker = ReplyDedupTracker::new();
        tracker.record_reply(1, 100, "好的我知道了");
        assert!(tracker.has_recent_similar_reply(1, "好的我知道了", 0.9));
    }
}
