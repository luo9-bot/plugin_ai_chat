#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""风格神经元离线训练器

从运行端留档的 SQLite（data/mind/archive/{群号}/messages.db + replies.db）学习
她的回复风格条件分布，产出三类产物：

1. {gid}.nn.json   —— 神经元权重：hashing trick 特征 → 单隐层 MLP（tanh）
                      → 多任务 softmax 头（长度档/分泡数/语气/句式）。
                      Rust 运行端同构前向推理（mind/style.rs），CPU 微秒级。
2. {gid}.style.json —— 语言指纹语料：群友口头禅、她的高频句式、长度分布。
3. {gid}.sft.jsonl  —— (触发, 回复) 配对数据集，供你用自己的底座做 LoRA SFT。

算法：Weinberger et al. 2009 hashing trick（免词表、维度固定）+
多层感知机反向传播（交叉熵 + L2 + SGD）。纯 numpy，无深度学习框架。

用法：
  python train.py --group 961949571 \
      --archive ../../data/mind/archive --out ../../data/mind/style
依赖：numpy（仅本脚本需要；运行端零依赖）
"""

import argparse
import hashlib
import json
import math
import os
import sqlite3
from collections import Counter

import numpy as np

D = 512      # 特征维度（hashing trick 目标空间）
H = 48       # 隐层宽度
EPOCHS = 24
LR = 0.06
L2 = 1e-4
SEED = 20260906

HEADS = ["length", "bubbles", "mood", "pattern"]


def h32(text: str) -> int:
    """特征哈希：与 Rust 端 mind/style.rs 同构（sha1 前 4 字节）"""
    return int.from_bytes(hashlib.sha1(text.encode("utf-8")).digest()[:4], "big")


def features(trigger: str, user_id: int, hour: int) -> list:
    """稀疏特征索引：偏置 + 字符 2/3-gram + 时段槽 + 人物槽"""
    idx = {0}
    compact = "".join(trigger.split())
    for n in (2, 3):
        for i in range(len(compact) - n + 1):
            idx.add(h32(compact[i:i + n]) % (D - 16) + 1)
    idx.add(D - 16 + hour % 8)                # 时段槽 D-16..D-9
    idx.add(D - 8 + h32(f"u{user_id}") % 8)   # 人物槽 D-8..D-1
    return sorted(idx)


def length_label(reply: str) -> str:
    n = len(reply.replace("|^|", "").strip())
    return "短" if n <= 10 else ("中" if n <= 30 else "长")


def bubbles_label(reply: str) -> str:
    return ["单泡", "两泡", "多泡"][min(reply.count("|^|"), 2)]


def mood_label(reply: str) -> str:
    if "！" in reply or "!!" in reply:
        return "热烈"
    if "~" in reply or reply.endswith("呢"):
        return "软"
    return "平"


def derive_patterns(replies, top_k=16, min_count=3):
    """句式模板：每条回复的第一段（|前）中高频出现的开头"""
    counter = Counter()
    for reply in replies:
        first = reply.split("|^|")[0].strip()
        if 2 <= len(first) <= 14:
            counter[first] += 1
    return [p for p, c in counter.most_common(top_k) if c >= min_count]


def pattern_label(reply, patterns):
    first = reply.split("|^|")[0].strip()
    return first if first in patterns else "其他"


def load_pairs(group_id: int, archive_dir: str):
    """从 SQLite 读 (触发, 回复) 配对 + 群友语料"""
    db_dir = os.path.join(archive_dir, str(group_id))
    replies_db = os.path.join(db_dir, "replies.db")
    messages_db = os.path.join(db_dir, "messages.db")
    if not os.path.exists(replies_db):
        raise SystemExit(f"未找到 {replies_db}——先让 bot 跑一段时间积累留档")

    conn = sqlite3.connect(replies_db)
    rows = conn.execute(
        "SELECT ts, user_id, trigger_content, reply_content "
        "FROM replies ORDER BY ts ASC"
    ).fetchall()
    conn.close()

    corpus = []
    if os.path.exists(messages_db):
        conn = sqlite3.connect(messages_db)
        corpus = [r[0] for r in conn.execute("SELECT content FROM messages").fetchall()]
        conn.close()
    return rows, corpus


def train(pairs):
    """单隐层 MLP + 多任务 softmax 头，教科书反向传播"""
    rng = np.random.default_rng(SEED)

    replies = [r for (_, _, _, r) in pairs]
    patterns = derive_patterns(replies)
    classes = {
        "length": ["短", "中", "长"],
        "bubbles": ["单泡", "两泡", "多泡"],
        "mood": ["热烈", "软", "平"],
        "pattern": patterns + ["其他"],
    }
    slices, offset = {}, 0
    for head in HEADS:
        slices[head] = (offset, len(classes[head]))
        offset += len(classes[head])
    total_classes = offset

    label_fn = {
        "length": length_label,
        "bubbles": bubbles_label,
        "mood": mood_label,
        "pattern": lambda r: pattern_label(r, patterns),
    }

    samples = []
    for (ts, user_id, trigger, reply) in pairs:
        hour = (ts + 8 * 3600) % 86400 // 3600
        x = features(trigger, user_id, hour)
        y = {}
        for head in HEADS:
            label = label_fn[head](reply)
            y[head] = classes[head].index(label)
        samples.append((x, y, trigger, reply, user_id, hour))

    split = max(1, int(len(samples) * 0.9))
    train_set, valid_set = samples[:split], samples[split:]

    W1 = rng.normal(0, 0.08, (H, D))
    b1 = np.zeros(H)
    W2 = rng.normal(0, 0.08, (total_classes, H))
    b2 = np.zeros(total_classes)

    def forward(x_idx):
        z1 = W1[:, x_idx].sum(axis=1) + b1          # 稀疏前向
        h = np.tanh(z1)
        z2 = W2 @ h + b2
        return z1, h, z2

    def softmax(z):
        e = np.exp(z - z.max())
        return e / e.sum()

    def head_accuracy(dataset):
        correct = {head: 0 for head in HEADS}
        for (x, y, *_ ) in dataset:
            _, _, z2 = forward(x)
            for head in HEADS:
                lo, ln = slices[head]
                pred = int(np.argmax(z2[lo:lo + ln]))
                correct[head] += int(pred == y[head])
        n = max(1, len(dataset))
        return {head: correct[head] / n for head in HEADS}

    for epoch in range(EPOCHS):
        rng.shuffle(train_set)
        total_loss = 0.0
        lr = LR * (0.5 ** (epoch // 8))  # 阶梯衰减
        for (x_idx, y, *_ ) in train_set:
            z1, h, z2 = forward(x_idx)
            x_vec = np.zeros(D)
            x_vec[x_idx] = 1.0

            dz2 = np.zeros(total_classes)
            for head in HEADS:
                lo, ln = slices[head]
                prob = softmax(z2[lo:lo + ln])
                onehot = np.zeros(ln)
                onehot[y[head]] = 1.0
                dz2[lo:lo + ln] = prob - onehot
                total_loss -= math.log(max(prob[y[head]], 1e-9))

            gW2 = np.outer(dz2, h) + L2 * W2
            gb2 = dz2
            gh = W2.T @ dz2 * (1.0 - h * h)

            W2 -= lr * gW2
            b2 -= lr * gb2
            # W1 稀疏更新：本样本只有 x_idx 列被前向触及
            W1[:, x_idx] -= lr * gh[:, None] + lr * L2 * W1[:, x_idx]
            b1 -= lr * gh

        if epoch % 8 == 7:
            acc = head_accuracy(valid_set or train_set)
            print(f"epoch {epoch + 1:3d}  loss {total_loss / max(1, len(train_set)):.4f}  "
                  + "  ".join(f"{k}={v:.2f}" for k, v in acc.items()))

    print("最终验证集准确率：", head_accuracy(valid_set or train_set))
    return W1, b1, W2, b2, classes, slices, patterns, train_set, valid_set


def export_nn(path, W1, b1, W2, b2, classes):
    def mat(a):
        return [[round(float(v), 4) for v in row] for row in a]

    heads = [{"name": h, "classes": classes[h]} for h in HEADS]
    with open(path, "w", encoding="utf-8") as f:
        json.dump({
            "version": 1,
            "feature_dim": D,
            "hidden": H,
            "heads": heads,
            "w1": mat(W1),
            "b1": [round(float(v), 4) for v in b1],
            "w2": mat(W2),
            "b2": [round(float(v), 4) for v in b2],
        }, f, ensure_ascii=False)


def export_style(path, group_id, corpus, replies, patterns):
    """语言指纹语料：运行端注入表达框架的「这个群怎么说话」素材"""
    def ngrams(text, n):
        compact = "".join(text.split())
        return [compact[i:i + n] for i in range(len(compact) - n + 1)]

    # 群友口头禅：在群消息语料中显著高频、且不在常用字背景里的 2-4 字片段
    body = Counter()
    for msg in corpus:
        body.update(ngrams(msg, 2))
        body.update(ngrams(msg, 3))
    bg = Counter()
    for reply in replies:
        bg.update(ngrams(reply, 2))
        bg.update(ngrams(reply, 3))
    catchphrases = [
        {"text": g, "count": c}
        for g, c in body.most_common(200)
        if c >= 5 and len(g.strip()) >= 2 and bg.get(g, 0) < c * 0.6
    ][:24]

    length_dist = Counter(length_label(r) for r in replies)
    with open(path, "w", encoding="utf-8") as f:
        json.dump({
            "group_id": group_id,
            "corpus_size": len(corpus),
            "reply_count": len(replies),
            "patterns": patterns,
            "catchphrases": catchphrases,
            "length_dist": dict(length_dist),
        }, f, ensure_ascii=False, indent=1)


def export_sft(path, pairs):
    """SFT 数据集：用户用自己的底座做微调的原料"""
    system = "（此处替换为洛玖的人设——训练前改成你的 persona 文本）"
    with open(path, "w", encoding="utf-8") as f:
        for (_, _, trigger, reply) in pairs:
            record = {
                "messages": [
                    {"role": "system", "content": system},
                    {"role": "user", "content": trigger},
                    {"role": "assistant", "content": reply},
                ]
            }
            f.write(json.dumps(record, ensure_ascii=False) + "\n")


def main():
    parser = argparse.ArgumentParser(description="风格神经元离线训练器")
    parser.add_argument("--group", type=int, required=True, help="群号")
    parser.add_argument("--archive", default="../../data/mind/archive", help="留档目录")
    parser.add_argument("--out", default="../../data/mind/style", help="产物输出目录")
    args = parser.parse_args()

    pairs, corpus = load_pairs(args.group, args.archive)
    corpus = corpus if corpus else []
    print(f"配对样本 {len(pairs)} 条，群消息语料 {len(corpus)} 条")
    if len(pairs) < 30:
        raise SystemExit("配对样本太少（<30）——让 bot 再跑几天，数据够了再来训练")

    W1, b1, W2, b2, classes, slices, patterns, train_set, valid_set = train(pairs)

    os.makedirs(args.out, exist_ok=True)
    export_nn(os.path.join(args.out, f"{args.group}.nn.json"), W1, b1, W2, b2, classes)
    export_style(os.path.join(args.out, f"{args.group}.style.json"),
                 args.group, corpus, [r for (*_, r) in pairs], patterns)
    export_sft(os.path.join(args.out, f"{args.group}.sft.jsonl"), pairs)

    weights_kb = os.path.getsize(os.path.join(args.out, f"{args.group}.nn.json")) // 1024
    print(f"已产出：{args.group}.nn.json（{weights_kb} KB）/ "
          f"{args.group}.style.json / {args.group}.sft.jsonl")
    print("把这三个文件放到运行端 data/mind/style/ 下即生效。")


if __name__ == "__main__":
    main()
