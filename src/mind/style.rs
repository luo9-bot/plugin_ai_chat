//! 风格神经元：离线训练产物在运行端的推理侧
//!
//! 权重由 tools/style_trainer/train.py 产出（单隐层 MLP：tanh 隐层 +
//! 多任务 softmax 头，hashing trick 特征）。本模块用**同构特征哈希**
//! （sha1 前 4 字节，与 Python 端 hashlib.sha1 一致）重建输入，
//! 手写稀疏前向——48×512 规模 ≈ 3 万次乘加，微秒级，零框架零 GPU。
//!
//! 输出是"这一轮的手感"：从她自己的真实回复记录里学来的统计先验，
//! 以参考语气的形式进入感官——内容由她生成，先验来自数据。
//! 没有训练产物时整条链路静默缺席，不影响其余系统。

use serde::Deserialize;
use std::collections::HashSet;
use tracing::warn;

use crate::config;

// ── 权重模型 ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct HeadDef {
    pub name: String,
    pub classes: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StyleNet {
    pub feature_dim: usize,
    pub hidden: usize,
    pub heads: Vec<HeadDef>,
    pub w1: Vec<Vec<f32>>,
    pub b1: Vec<f32>,
    pub w2: Vec<Vec<f32>>,
    pub b2: Vec<f32>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct StyleCorpus {
    pub patterns: Vec<String>,
    pub catchphrases: Vec<serde_json::Value>,
}

/// 与 Python 端 hashlib.sha1 同构的特征哈希
fn h32(text: &str) -> usize {
    use sha1::{Digest, Sha1};
    let mut hasher = Sha1::new();
    hasher.update(text.as_bytes());
    let digest = hasher.finalize();
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]) as usize
}

/// 稀疏特征索引：偏置 + 字符 2/3-gram + 时段槽 + 人物槽（与 train.py 一致）
fn features(trigger: &str, user_id: u64, hour: u32, dim: usize) -> Vec<usize> {
    let mut idx: HashSet<usize> = HashSet::from([0]);
    // 槽位区需要 dim > 16（合成小模型测试时只保留 gram 区）
    let gram_mod = dim.saturating_sub(16);
    let compact: String = trigger.split_whitespace().collect();
    let chars: Vec<char> = compact.chars().collect();
    if gram_mod > 0 {
        for n in [2usize, 3] {
            for window in chars.windows(n) {
                let gram: String = window.iter().collect();
                idx.insert(h32(&gram) % gram_mod + 1);
            }
        }
        if dim >= 16 {
            idx.insert(dim - 16 + (hour % 8) as usize);
            idx.insert(dim - 8 + h32(&format!("u{user_id}")) % 8);
        }
    }
    idx.into_iter().collect()
}

/// 前向推理：返回各头的 (标签, 概率)
fn forward(net: &StyleNet, trigger: &str, user_id: u64, hour: u32) -> Vec<(String, String, f32)> {
    let x = features(trigger, user_id, hour, net.feature_dim);

    // 稀疏隐层：h = tanh(Σ W1[:, i] * 1 + b1)
    let hidden: Vec<f32> = (0..net.hidden)
        .map(|t| {
            let mut z = net.b1[t];
            for &i in &x {
                if let Some(row) = net.w1.get(t)
                    && let Some(v) = row.get(i)
                {
                    z += v;
                }
            }
            z.tanh()
        })
        .collect();

    // 输出层：按 head 分段 softmax
    let logits: Vec<f32> = net
        .w2
        .iter()
        .zip(&net.b2)
        .map(|(row, b)| {
            row.iter()
                .zip(hidden.iter())
                .map(|(w, h)| w * h)
                .sum::<f32>()
                + b
        })
        .collect();

    let mut out: Vec<(String, String, f32)> = Vec::new();
    let mut offset = 0usize;
    for head in &net.heads {
        let len = head.classes.len();
        let slice = &logits[offset..offset + len];
        let max = slice.iter().cloned().fold(f32::MIN, f32::max);
        let exps: Vec<f32> = slice.iter().map(|z| (z - max).exp()).collect();
        let sum: f32 = exps.iter().sum();
        let mut best: Option<(usize, f32)> = None;
        for (i, p) in exps.iter().enumerate() {
            if best.is_none_or(|(_, bp)| *p > bp) {
                best = Some((i, *p));
            }
        }
        if let Some((i, p)) = best {
            out.push((head.name.clone(), head.classes[i].clone(), p / sum));
        }
        offset += len;
    }
    out
}

// ── 加载与注入 ──────────────────────────────────────────────────

fn style_dir() -> std::path::PathBuf {
    config::data_dir().join("mind").join("style")
}

/// 这一番的手感语块（voice 的 user_content 追加项）
pub(crate) fn context_block(group_id: u64, trigger: &str, user_id: u64) -> Option<String> {
    let nn_path = style_dir().join(format!("{group_id}.nn.json"));
    let Ok(nn_text) = std::fs::read_to_string(&nn_path) else {
        return None;
    };
    let net: StyleNet = match serde_json::from_str(&nn_text) {
        Ok(net) => net,
        Err(e) => {
            warn!(error = %e, "style: 权重解析失败，跳过风格先验");
            return None;
        }
    };

    let hour = crate::util::current_hour_cst();
    let predictions = forward(&net, trigger, user_id, hour);
    if predictions.is_empty() {
        return None;
    }

    let mut sections: Vec<String> = Vec::new();
    let parts: Vec<String> = predictions
        .iter()
        .map(|(name, class, prob)| format!("{name}:{class}({prob:.2})"))
        .collect();
    sections.push(parts.join(" ｜ "));

    // 语言指纹语料（可选文件）
    let style_path = style_dir().join(format!("{group_id}.style.json"));
    if let Ok(text) = std::fs::read_to_string(&style_path)
        && let Ok(corpus) = serde_json::from_str::<StyleCorpus>(&text)
    {
        if !corpus.patterns.is_empty() {
            sections.push(format!("你常用的句式：{}", corpus.patterns.join(" / ")));
        }
        let phrases: Vec<String> = corpus
            .catchphrases
            .iter()
            .filter_map(|c| c.get("text").and_then(|v| v.as_str()))
            .take(8)
            .map(str::to_string)
            .collect();
        if !phrases.is_empty() {
            sections.push(format!("这个群里大家常这么说：{}", phrases.join("、")));
        }
    }

    Some(format!(
        "# 这一轮的手感（从你自己的回复记录里学来的，仅供参考，不是命令）\n{}",
        sections.join("\n")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feature_hash_matches_boundaries() {
        // 特征索引必须落在槽位区间内（0 偏置 / gram / 时段 / 人物）
        let idx = features("你好呀", 42, 20, 512);
        assert!(idx.contains(&0));
        assert!(idx.iter().all(|&i| i < 512));
        // 时段槽：20 点 → 512-16+4=500
        assert!(idx.contains(&(512 - 16 + 20 % 8)));
    }

    #[test]
    fn forward_on_synthetic_weights() {
        let net: StyleNet = serde_json::from_str(
            r#"{
                "version": 1, "feature_dim": 16, "hidden": 2,
                "heads": [{"name": "length", "classes": ["短", "长"]}],
                "w1": [[0.5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
                       [0, 0.5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]],
                "b1": [0.0, 0.0],
                "w2": [[1.0, 0.0], [-1.0, 0.0]],
                "b2": [0.0, 0.0]
            }"#,
        )
        .unwrap();
        let out = forward(&net, "anything", 1, 0);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "length");
        assert!(out[0].2 > 0.5);
    }
}
