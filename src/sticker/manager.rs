//! 洛玖表情包管理器
//!
//! 负责表情包的注册、选择和维护。
//! 使用视觉模型（VLM）进行表情包选择和描述生成。

use tracing::{debug, info, warn};
use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::store::*;

/// 表情包选择结果
pub struct StickerSelection {
    pub hash: String,
    pub path: String,
    pub description: String,
    pub reason: String,
}

// ── 注册 ────────────────────────────────────────────────────────

/// 注册表情包（从用户发送的图片）
///
/// 流程：哈希去重 → 保存文件 → VLM 生成描述 → 原子注册
pub fn register_sticker(image_bytes: &[u8], format: &str) -> Option<String> {
    let hash = compute_hash(image_bytes);

    // 去重检查（持锁）
    {
        let store = load_store();
        if store.stickers.iter().any(|e| e.hash == hash) {
            debug!(hash = %hash[..16.min(hash.len())], "sticker: already registered");
            return None;
        }
    }

    // 保存文件
    let dir = sticker_dir();
    std::fs::create_dir_all(&dir).ok();
    let filename = format!("{}.{}", hash, format);
    let path = dir.join(&filename);
    std::fs::write(&path, image_bytes).ok();

    // 通过 VLM 生成描述和情绪标签
    let (description, emotions) = generate_description_with_vlm(&path);

    let now = crate::util::now_secs();
    let entry = StickerEntry {
        hash: hash.clone(),
        path: format!("sticker/{}", filename),
        description: description.clone(),
        emotions,
        query_count: 0,
        is_registered: true,
        is_banned: false,
        is_builtin: false,
        registered_at: now,
        last_used_at: now,
    };

    info!(hash = %hash[..16.min(hash.len())], description = %description, "sticker: registered");

    // 原子性添加到存储（防止并发竞态）
    add_entry_and_save(entry);
    Some(hash)
}

/// 从 CQ 码中提取图片 URL 并注册（仅表情包，不注册普通图片）
///
/// 区分方式：
/// - sub_type=1 + summary=[动画表情] → 表情包，自动注册
/// - sub_type=0 → 普通图片，跳过
pub fn register_from_cq(cq_message: &str) -> Option<String> {
    // 只处理表情包，跳过普通图片
    if !is_sticker_cq(cq_message) {
        debug!("sticker: not an sticker CQ code, skipping registration");
        return None;
    }

    let urls = crate::vision::extract_image_urls(cq_message);
    for url in &urls {
        // 下载图片
        if let Ok(mut resp) = ureq::get(url).call()
            && let Ok(bytes) = resp.body_mut().read_to_vec() {
                let format = detect_format(&bytes);

                // 内容过滤
                if !content_filtration(&bytes, &format) {
                    warn!("sticker: content filtration rejected");
                    return None;
                }

                if let Some(hash) = register_sticker(&bytes, &format) {
                    // 获取注册的表情包描述并缓存到 URL
                    let store = load_store();
                    if let Some(entry) = store.stickers.iter().find(|e| e.hash == hash) {
                        let desc = entry.description.clone();
                        super::store::cache_url_description(url, &desc);
                        debug!(url = %url, desc = %desc, "sticker: cached URL description");
                    }
                    return Some(hash);
                }
            }
    }
    None
}

/// 判断 CQ 码是否为表情包（而非普通图片）
///
/// 表情包特征：sub_type=1，或 summary 包含 [动画表情]
pub fn is_sticker_cq(cq_message: &str) -> bool {
    // sub_type=1 表示表情包
    if cq_message.contains("sub_type=1") {
        return true;
    }
    // summary=[动画表情]（HTML 转义形式 &#91;动画表情&#93;）
    if cq_message.contains("&#91;动画表情&#93;") || cq_message.contains("[动画表情]") {
        return true;
    }
    false
}

// ── 选择 ────────────────────────────────────────────────────────

/// 使用 VLM 网格选择最合适的表情包
///
/// 1. 从候选中随机采样
/// 2. 加载图片
/// 3. 发送给 VLM 让它选择
/// 4. 解析选择结果
pub fn select_sticker_vlm(
    context: &str,
    target_emotion: &str,
    exclude_hashes: &[String],
) -> Option<StickerSelection> {
    let store = load_store();
    let candidates: Vec<&StickerEntry> = store.stickers.iter()
        .filter(|e| e.is_registered && !e.is_banned)
        .filter(|e| !exclude_hashes.contains(&e.hash))
        .collect();

    if candidates.is_empty() {
        return None;
    }

    let sample_size = candidates.len().min(25);
    let sampled = weighted_sample(&candidates, sample_size);

    // 加载图片路径
    let data_dir = crate::config::data_dir();
    let image_paths: Vec<String> = sampled.iter()
        .map(|e| data_dir.join(&e.path).to_string_lossy().to_string())
        .collect();

    // 构建 VLM 请求：多张图片 + 选择 prompt
    let selection = select_with_vlm(&image_paths, context, target_emotion);

    match selection {
        Some((index, reason)) => {
            if index < sampled.len() {
                let selected = sampled[index];
                debug!(
                    hash = %selected.hash[..16.min(selected.hash.len())],
                    emotion = target_emotion,
                    reason = %reason,
                    "sticker: vlm selected"
                );
                Some(StickerSelection {
                    hash: selected.hash.clone(),
                    path: selected.path.clone(),
                    description: selected.description.clone(),
                    reason,
                })
            } else {
                // 索引越界，fallback 到第一个
                let selected = sampled[0];
                Some(StickerSelection {
                    hash: selected.hash.clone(),
                    path: selected.path.clone(),
                    description: selected.description.clone(),
                    reason: "VLM 索引越界，使用默认".to_string(),
                })
            }
        }
        None => {
            // VLM 失败，fallback 到情绪标签匹配
            select_sticker_by_emotion(target_emotion, &candidates)
        }
    }
}

/// 情绪标签匹配选择（VLM 的 fallback）
fn select_sticker_by_emotion(
    target_emotion: &str,
    candidates: &[&StickerEntry],
) -> Option<StickerSelection> {
    let mut scored: Vec<(&StickerEntry, f64)> = candidates.iter()
        .map(|e| {
            let emotion_score = calculate_emotion_similarity(target_emotion, &e.emotions);
            let usage_bonus = (e.query_count as f64).ln().max(0.0) * 0.1;
            (*e, emotion_score + usage_bonus)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let top_n: Vec<_> = scored.into_iter().take(10).collect();
    if top_n.is_empty() {
        return None;
    }

    let idx = (crate::util::now_millis() as usize) % top_n.len();
    let (selected, _) = &top_n[idx];

    Some(StickerSelection {
        hash: selected.hash.clone(),
        path: selected.path.clone(),
        description: selected.description.clone(),
        reason: format!("情绪标签匹配: {}", target_emotion),
    })
}

// ── VLM 调用 ────────────────────────────────────────────────────

/// 通过 VLM 生成表情包描述和情绪标签
fn generate_description_with_vlm(path: &std::path::Path) -> (String, Vec<String>) {
    let cfg = crate::config::get();
    if !cfg.vision.enabled() {
        return ("表情包".to_string(), vec!["未知".to_string()]);
    }

    let prompt = "这是一个表情包图片。请提取该表情主要表达的情绪或语气标签，最多5个，用逗号分隔。只返回标签，不要解释。\n例如：惊讶、疑惑、茫然、错愕、震惊、惊恐、傻眼、呆滞、蒙圈、愣住、一脸懵、难以置信、瞳孔地震、兴奋、元气、激动、开心、搞怪、欢喜、得意、偷笑、坏笑、满足、窃喜、暗爽、憨笑、得瑟、生气、无奈、红温、愠怒、暴怒、炸毛、咬牙切齿、无能狂怒、血压飙升、害羞、窘迫、慌乱、难为情、紧张、脸红、社死、脚趾扣地、伤心、委屈、破防、崩溃、emo、心累、欲哭无泪、嫌弃、鄙夷、白眼、无语、阴阳怪气、不耐烦、烦躁、抓狂、生无可恋、恐惧、吓尿、心态炸裂、当场去世、我人没了、傲娇、贱萌、叉腰、可把我牛逼坏了、暗中观察、慵懒、摆烂、躺平、葛优躺、敷衍、毁灭吧、感动、泪目、扎心、暖到了、又好笑又好哭...";

    match call_vlm_with_image(path, prompt) {
        Some(response) => {
            let emotions = parse_emotions(&response);
            let description = emotions.join(",");
            (description, emotions)
        }
        None => ("表情包".to_string(), vec!["未知".to_string()]),
    }
}

/// 内容过滤：通过 VLM 检查表情包是否合规
///
/// 1. 符合公序良俗
/// 2. 不能是色情、暴力等违法内容
/// 3. 不能是截图、聊天记录
/// 4. 不要出现 5 个以上文字
fn content_filtration(image_bytes: &[u8], format: &str) -> bool {
    let cfg = crate::config::get();
    if !cfg.vision.enabled() {
        // 无 VLM 时跳过过滤
        return true;
    }

    // 保存临时文件
    let temp_dir = std::env::current_dir()
        .unwrap_or_default()
        .join("data").join("plugin_ai_chat").join("temp");
    std::fs::create_dir_all(&temp_dir).ok();

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_path = temp_dir.join(format!("sticker_filtration_{}.{}", timestamp, format));
    debug!("文件写入路径: {:?}", tmp_path);
    std::fs::write(&tmp_path, image_bytes).ok();

    let prompt = if format == "gif" {
        format!(
            "这是一个动态图表情包，每一张图代表了动态图的一帧。{}",
            crate::prompt::PromptManager::get()
            .raw("sticker_content_filtration")
        )
    }
    else {
        crate::prompt::PromptManager::get()
            .raw("sticker_content_filtration")
            .to_string()
    };

    let result = call_vlm_with_image(&tmp_path, &prompt);

    match result {
        Some(response) => {
            let trimmed = response.trim();
            // 只接受"是"作为通过
            let passed = trimmed.contains("是") && !trimmed.contains("否");
            if !passed {
                info!(response = %trimmed, "sticker: content filtration rejected");
            }
            std::fs::remove_file(&tmp_path).ok();
            passed
        }
        None => {
            // VLM 调用失败，保守起见拒绝
            warn!("sticker: content filtration VLM call failed, rejecting");
            std::fs::remove_file(&tmp_path).ok();
            false
        }
    }
}

/// 使用 VLM 从多个候选中选择最佳表情包
///
/// 使用 VLM 网格选择最合适的表情包
fn select_with_vlm(
    image_paths: &[String],
    context: &str,
    target_emotion: &str,
) -> Option<(usize, String)> {
    let cfg = crate::config::get();
    if !cfg.vision.enabled() || image_paths.is_empty() {
        return None;
    }

    let grid_bytes = match create_grid_image(image_paths) {
        Some(bytes) => bytes,
        None => {
            warn!("sticker: grid creation failed, falling back to single image");
            std::fs::read(&image_paths[0]).unwrap_or_default()
        }
    };

    // 使用 base64 编码网格图（不依赖文件系统）
    let base64_image = STANDARD.encode(&grid_bytes);

    // 构建 VLM 请求：单张网格图 + 选择 prompt
    let (cols, rows) = calculate_grid_shape(image_paths.len());
    let prompt = format!(
        "你是洛玖的临时表情包选择子代理。\n\n\
         当前对话上下文：\n{}\n\n\
         目标情绪：{}\n\n\
         上面是 {} 个候选表情包，排列成 {}×{} 网格（按顺序编号 1-{}）。\n\
         请根据对话情境和目标情绪，选择最合适的一个。\n\
         只返回 JSON：{{\"index\": N, \"reason\": \"选择原因\"}}\n\
         N 为候选列表中的序号（从 1 开始）。",
        context, target_emotion, image_paths.len(), cols, rows, image_paths.len()
    );

    let request_body = serde_json::json!({
        "model": cfg.vision.model,
        "input": [{
            "role": "user",
            "content": [
                { "type": "input_image", "image_url": format!("data:image/png;base64,{}", base64_image) },
                { "type": "input_text", "text": prompt }
            ]
        }],
        "max_output_tokens": 256
    });

    let url = format!("{}/responses", cfg.vision.base_url.trim_end_matches('/'));

    let result = call_vlm_api(&url, &cfg.vision.api_key, &request_body);

    match result {
        Some(response) => {
            if let Some(json_str) = crate::ai::extract_json(&response)
                && let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    let index = parsed.get("index")
                        .and_then(|v| v.as_u64())
                        .map(|n| (n as usize).saturating_sub(1))
                        .unwrap_or(0);
                    let reason = parsed.get("reason")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    return Some((index, reason));
                }
            if let Some(n) = extract_number(&response) {
                return Some((n.saturating_sub(1), "从文本提取".to_string()));
            }
            None
        }
        None => None,
    }
}

/// 将多张图片拼成网格图
///
/// - 每张图缩放到 256x256 格子
/// - 格子之间 12px 间距
/// - 左上角绘制序号角标
fn create_grid_image(image_paths: &[String]) -> Option<Vec<u8>> {
    use image::{ImageBuffer, Rgb, RgbImage};

    let count = image_paths.len();
    if count == 0 {
        return None;
    }

    // 计算网格尺寸（接近正方形）
    let (cols, rows) = calculate_grid_shape(count);
    let tile_size: u32 = 256;
    let gap: u32 = 12;

    let canvas_w = cols * tile_size + (cols + 1) * gap;
    let canvas_h = rows * tile_size + (rows + 1) * gap;

    // 创建白色画布
    let mut canvas: RgbImage = ImageBuffer::from_pixel(canvas_w, canvas_h, Rgb([255u8, 255, 255]));

    for (i, path) in image_paths.iter().enumerate() {
        let col = (i as u32) % cols;
        let row = (i as u32) / cols;

        let x = gap + col * (tile_size + gap);
        let y = gap + row * (tile_size + gap);

        // 加载并缩放图片
        if let Ok(img) = image::open(path) {
            let resized = img.resize_exact(tile_size, tile_size, image::imageops::FilterType::Lanczos3);
            let rgb = resized.to_rgb8();

            // 复制到画布
            for py in 0..tile_size {
                for px in 0..tile_size {
                    let pixel = rgb.get_pixel(px, py);
                    canvas.put_pixel(x + px, y + py, *pixel);
                }
            }

            // 绘制序号角标（简单实现：左上角黑色方块 + 白色数字）
            draw_number_badge(&mut canvas, x + 14, y + 14, i + 1);
        }
    }

    // 编码为 PNG
    let mut buf = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new(&mut buf);
    image::ImageEncoder::write_image(encoder, canvas.as_raw(), canvas.width(), canvas.height(), image::ExtendedColorType::Rgb8).ok()?;
    Some(buf)
}

/// 计算网格形状（接近正方形）
fn calculate_grid_shape(count: usize) -> (u32, u32) {
    let n = count as u32;
    let sqrt = (n as f64).sqrt() as u32;
    for cols in (1..=sqrt + 1).rev() {
        let rows = n.div_ceil(cols);
        if cols * rows >= n && (cols as i32 - rows as i32).abs() <= 1 {
            return (cols, rows);
        }
    }
    (n.min(5), n.div_ceil(5))
}

/// 在画布上绘制数字角标（简化的像素字体）
fn draw_number_badge(canvas: &mut image::RgbImage, x: u32, y: u32, num: usize) {
    let badge_size = 28u32;
    // 黑色半透明背景
    for dy in 0..badge_size {
        for dx in 0..badge_size {
            if x + dx < canvas.width() && y + dy < canvas.height() {
                canvas.put_pixel(x + dx, y + dy, image::Rgb([40u8, 40, 40]));
            }
        }
    }
    // 白色数字（简化：用像素点绘制）
    let digits = format!("{}", num);
    let mut offset_x = 6u32;
    for ch in digits.chars() {
        draw_digit(canvas, x + offset_x, y + 6, ch);
        offset_x += 10;
    }
}

/// 绘制单个数字字符（5x7 像素字体）
fn draw_digit(canvas: &mut image::RgbImage, x: u32, y: u32, ch: char) {
    let patterns: &[u8] = match ch {
        '0' => &[0b111, 0b101, 0b101, 0b101, 0b101, 0b101, 0b111],
        '1' => &[0b010, 0b110, 0b010, 0b010, 0b010, 0b010, 0b111],
        '2' => &[0b111, 0b001, 0b001, 0b111, 0b100, 0b100, 0b111],
        '3' => &[0b111, 0b001, 0b001, 0b111, 0b001, 0b001, 0b111],
        '4' => &[0b101, 0b101, 0b101, 0b111, 0b001, 0b001, 0b001],
        '5' => &[0b111, 0b100, 0b100, 0b111, 0b001, 0b001, 0b111],
        '6' => &[0b111, 0b100, 0b100, 0b111, 0b101, 0b101, 0b111],
        '7' => &[0b111, 0b001, 0b001, 0b010, 0b010, 0b100, 0b100],
        '8' => &[0b111, 0b101, 0b101, 0b111, 0b101, 0b101, 0b111],
        '9' => &[0b111, 0b101, 0b101, 0b111, 0b001, 0b001, 0b111],
        _ => &[0b111, 0b101, 0b101, 0b101, 0b101, 0b101, 0b111],
    };

    for (dy, &row) in patterns.iter().enumerate() {
        for dx in 0..3 {
            if (row >> (2 - dx)) & 1 == 1 {
                let px = x + dx * 2;
                let py = y + dy as u32 * 2;
                if px + 1 < canvas.width() && py + 1 < canvas.height() {
                    canvas.put_pixel(px, py, image::Rgb([255u8, 255, 255]));
                    canvas.put_pixel(px + 1, py, image::Rgb([255u8, 255, 255]));
                    canvas.put_pixel(px, py + 1, image::Rgb([255u8, 255, 255]));
                    canvas.put_pixel(px + 1, py + 1, image::Rgb([255u8, 255, 255]));
                }
            }
        }
    }
}

fn call_vlm_with_image(image_path: &std::path::Path, prompt: &str) -> Option<String> {
    let cfg = crate::config::get();
    if !cfg.vision.enabled() {
        return None;
    }

    // 读取文件
    let image_bytes = match std::fs::read(image_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            warn!("读取图片文件失败: {}", e);
            return None;
        }
    };

    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let base64_image = STANDARD.encode(&image_bytes);

    // 根据扩展名推断 MIME 类型
    let mime_type = image_path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| match ext.to_lowercase().as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            _ => "image/png",
        })
        .unwrap_or("image/png");

    let request_body = serde_json::json!({
        "model": cfg.vision.model,
        "input": [{
            "role": "user",
            "content": [
                {
                    "type": "input_image",
                    "image_url": format!("data:{};base64,{}", mime_type, base64_image)
                },
                {
                    "type": "input_text",
                    "text": prompt
                }
            ]
        }],
        "max_output_tokens": cfg.vision.max_tokens
    });

    let url = format!("{}/responses", cfg.vision.base_url.trim_end_matches('/'));
    call_vlm_api(&url, &cfg.vision.api_key, &request_body)
}

/// 通用 VLM API 调用
fn call_vlm_api(url: &str, api_key: &str, body: &serde_json::Value) -> Option<String> {
    let json_body = serde_json::to_string(body).ok()?;

    let mut resp = ureq::post(url)
        .header("Authorization", &format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .send(json_body.as_bytes())
        .ok()?;

    let resp_str = resp.body_mut().read_to_string().ok()?;

    // 解析响应
    serde_json::from_str::<serde_json::Value>(&resp_str).ok().and_then(|v| {
        // responses 格式
        v.get("output").and_then(|o| o.as_array()).and_then(|output| {
            output.iter().find_map(|item| {
                item.get("content").and_then(|c| c.as_array()).and_then(|contents| {
                    contents.iter().find_map(|content| {
                        content.get("text").and_then(|t| t.as_str()).map(|s| s.to_string())
                    })
                })
            })
        })
        // chat completions 格式
        .or_else(|| {
            v.get("choices").and_then(|c| c.as_array()).and_then(|choices| {
                choices.first().and_then(|c| {
                    c.get("message").and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str()).map(|s| s.to_string())
                })
            })
        })
    })
}

// ── 工具函数 ────────────────────────────────────────────────────

/// 更新表情包使用次数
pub fn update_usage(hash: &str) {
    let mut store = load_store();
    if let Some(entry) = store.stickers.iter_mut().find(|e| e.hash == hash) {
        entry.query_count += 1;
        entry.last_used_at = crate::util::now_secs();
        save_store(&store);
    }
}

/// 获取表情包文件路径
pub fn get_sticker_path(hash: &str) -> Option<String> {
    let store = load_store();
    store.stickers.iter()
        .find(|e| e.hash == hash && e.is_registered && !e.is_banned)
        .map(|e| crate::config::data_dir().join(&e.path).to_string_lossy().to_string())
}

/// 获取表情包统计
pub fn get_stats() -> (usize, usize) {
    let store = load_store();
    let total = store.stickers.len();
    let registered = store.stickers.iter().filter(|e| e.is_registered && !e.is_banned).count();
    (total, registered)
}

/// 维护：清理无效条目
pub fn maintenance() {
    let mut store = load_store();
    let data_dir = crate::config::data_dir();
    let before = store.stickers.len();

    store.stickers.retain(|e| {
        let full_path = data_dir.join(&e.path);
        if e.is_registered && !full_path.exists() {
            warn!(hash = %e.hash[..16.min(e.hash.len())], "sticker: file missing, removing");
            false
        } else {
            true
        }
    });

    let removed = before - store.stickers.len();
    if removed > 0 {
        info!(removed, "sticker: maintenance cleaned");
        save_store(&store);
    }
}

/// 加权采样：使用次数高的更容易被选中
fn weighted_sample<'a>(candidates: &[&'a StickerEntry], n: usize) -> Vec<&'a StickerEntry> {
    if candidates.len() <= n {
        return candidates.to_vec();
    }

    // 简单实现：按使用次数加权随机选择
    let total_weight: f64 = candidates.iter().map(|e| (e.query_count as f64 + 1.0).sqrt()).sum();
    let mut selected = Vec::new();
    let mut used = std::collections::HashSet::new();

    for _ in 0..n {
        let mut roll = (crate::util::now_millis() as f64 / 1000.0) % 1.0;
        // 简单的伪随机
        roll = (roll * 7919.0) % 1.0;
        let mut cumulative = 0.0;
        for (i, candidate) in candidates.iter().enumerate() {
            if used.contains(&i) { continue; }
            let weight = (candidate.query_count as f64 + 1.0).sqrt();
            cumulative += weight / total_weight;
            if roll < cumulative {
                selected.push(*candidate);
                used.insert(i);
                break;
            }
        }
    }

    // 如果没选够，补上未选的
    for (i, candidate) in candidates.iter().enumerate() {
        if selected.len() >= n { break; }
        if !used.contains(&i) {
            selected.push(*candidate);
        }
    }

    selected
}

fn compute_hash(data: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}_{:08x}", h, data.len())
}

/// 从描述文本中解析情绪标签
fn parse_emotions(description: &str) -> Vec<String> {
    description
        .split([',', '，', '；', ';', '\n'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.len() <= 10)
        .collect()
}

/// 计算目标情绪与表情包情绪标签的匹配度
fn calculate_emotion_similarity(target: &str, emotions: &[String]) -> f64 {
    if emotions.is_empty() {
        return 0.1;
    }
    let target_lower = target.to_lowercase();
    let mut max_score: f64 = 0.0;

    for emotion in emotions {
        let emotion_lower = emotion.to_lowercase();
        if target_lower == emotion_lower {
            max_score = max_score.max(1.0);
        } else if target_lower.contains(&emotion_lower) || emotion_lower.contains(&target_lower) {
            max_score = max_score.max(0.7);
        } else {
            let overlap = text_similarity(&target_lower, &emotion_lower);
            max_score = max_score.max(overlap * 0.5);
        }
    }
    max_score
}

fn text_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    let chars_a: Vec<char> = a.chars().collect();
    let chars_b: std::collections::HashSet<char> = b.chars().collect();
    let overlap = chars_a.iter().filter(|c| chars_b.contains(c)).count();
    overlap as f64 / chars_a.len().max(1) as f64
}

/// 从文本中提取第一个数字
fn extract_number(text: &str) -> Option<usize> {
    let mut num = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            num.push(c);
        } else if !num.is_empty() {
            break;
        }
    }
    num.parse().ok()
}

/// 检测图片格式
fn detect_format(bytes: &[u8]) -> String {
    if bytes.len() >= 4 {
        if bytes[0] == 0x89 && bytes[1] == 0x50 { return "png".to_string(); }
        if bytes[0] == 0xFF && bytes[1] == 0xD8 { return "jpg".to_string(); }
        if bytes[0] == 0x47 && bytes[1] == 0x49 { return "gif".to_string(); }
        if bytes[0] == 0x52 && bytes[1] == 0x49 { return "webp".to_string(); }
    }
    "png".to_string()
}

// ── NeSticker 内置表情注册 ──────────────────────────────────────

/// 注册内置表情包（ne_sticker），跳过内容审查
///
/// 内置表情已经过用户人工筛选，不经过 VLM 内容过滤。
pub fn register_builtin_sticker(image_bytes: &[u8], format: &str) -> Option<String> {
    let hash = compute_hash(image_bytes);

    // 去重检查
    {
        let store = load_store();
        if store.stickers.iter().any(|e| e.hash == hash) {
            debug!(hash = %hash[..16.min(hash.len())], "builtin: already registered");
            return None;
        }
    }

    // 保存到内置目录
    let dir = super::store::builtin_sticker_dir();
    std::fs::create_dir_all(&dir).ok();
    let filename = format!("{}.{}", hash, format);
    let path = dir.join(&filename);
    std::fs::write(&path, image_bytes).ok();

    let (description, emotions) = generate_description_with_vlm(&path);

    let now = crate::util::now_secs();
    let entry = StickerEntry {
        hash: hash.clone(),
        path: format!("ne_sticker/{}", filename),
        description: description.clone(),
        emotions,
        query_count: 0,
        is_registered: true,
        is_banned: false,
        is_builtin: true,
        registered_at: now,
        last_used_at: now,
    };

    info!(hash = %hash[..16.min(hash.len())], description = %description, "builtin: registered");
    add_entry_and_save(entry);
    Some(hash)
}

/// 初始化内置表情包：只处理 ne_sticker/ 目录中新增的文件
///
/// 已注册过的文件通过路径比对跳过，不读文件、不计算哈希。
/// 在插件启动时自动调用。
pub fn init_ne_stickers() {
    let dir = super::store::builtin_sticker_dir();
    if !dir.exists() {
        std::fs::create_dir_all(&dir).ok();
        debug!("ne_sticker: created directory {:?}", dir);
        return;
    }

    // 从注册表中收集已记录的内置表情文件名
    let store = load_store();
    let known: std::collections::HashSet<String> = store.stickers.iter()
        .filter(|e| e.is_builtin)
        .filter_map(|e| {
            // path 格式为 "ne_sticker/{filename}"
            e.path.strip_prefix("ne_sticker/").map(|s| s.to_string())
        })
        .collect();
    drop(store);

    let mut registered = 0;
    let mut skipped = 0;

    for entry in std::fs::read_dir(&dir).ok().into_iter().flatten() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(f) => f.to_string(),
            None => continue,
        };

        // 已注册的跳过，不读文件不算哈希
        if known.contains(&filename) {
            skipped += 1;
            continue;
        }

        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();

        if register_builtin_sticker(&bytes, &ext).is_some() {
            registered += 1;
        }
    }

    if registered > 0 || skipped > 0 {
        info!(registered, skipped, "ne_sticker: initialization complete");
    }
}

// ── Steal Emoji 自动收集 ────────────────────────────────────────

/// 扫描 sticker/ 目录，自动注册未入库的表情包（steal_emoji）
///
/// 只处理那些文件存在但尚未注册的表情包图片。
pub fn steal_emoji_scan() -> usize {
    let dir = super::store::sticker_dir();
    if !dir.exists() {
        return 0;
    }

    let store = load_store();
    let known_paths: std::collections::HashSet<String> = store.stickers.iter()
        .map(|e| crate::config::data_dir().join(&e.path).to_string_lossy().to_string())
        .collect();

    let mut registered = 0;

    for entry in std::fs::read_dir(&dir).ok().into_iter().flatten() {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let path_str = path.to_string_lossy().to_string();
        if known_paths.contains(&path_str) {
            continue;
        }

        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        let ext = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("png")
            .to_lowercase();

        // 走标准注册流程（含内容审查）
        let hash = compute_hash(&bytes);
        let store = load_store();
        if store.stickers.iter().any(|e| e.hash == hash) {
            continue;
        }

        if !content_filtration(&bytes, &ext) {
            warn!("steal_emoji: content filtration rejected {:?}", path);
            continue;
        }

        if register_sticker_from_path(&path, &ext, &bytes).is_some() {
            registered += 1;
        }
    }

    if registered > 0 {
        info!(registered, "steal_emoji: auto-registered");
    }
    registered
}

/// 从文件路径注册表情包（steal_emoji 内部使用）
fn register_sticker_from_path(path: &std::path::Path, format: &str, bytes: &[u8]) -> Option<String> {
    let hash = compute_hash(bytes);
    let (description, emotions) = generate_description_with_vlm(path);

    let now = crate::util::now_secs();
    let filename = format!("{}.{}", hash, format);
    let relative = format!("sticker/{}", filename);

    // 如果文件已经在 sticker/ 目录下则不动，否则复制过去
    let dir = super::store::sticker_dir();
    let target = dir.join(&filename);
    if !target.exists() {
        std::fs::create_dir_all(&dir).ok();
        std::fs::write(&target, bytes).ok();
    }

    let entry = StickerEntry {
        hash: hash.clone(),
        path: relative,
        description: description.clone(),
        emotions,
        query_count: 0,
        is_registered: true,
        is_banned: false,
        is_builtin: false,
        registered_at: now,
        last_used_at: now,
    };

    add_entry_and_save(entry);
    info!(hash = %hash[..16.min(hash.len())], "steal_emoji: registered");
    Some(hash)
}

/// 淘汰最不常用的非内置表情包（do_replace）
///
/// 当注册的非内置表情包超过 max_reg_num 时，
/// 按使用次数升序 + 最后使用时间升序排序，淘汰超额的条目。
pub fn do_replace_eviction(max_reg_num: usize) -> usize {
    let mut store = load_store();
    let data_dir = crate::config::data_dir();

    let non_builtin_count = store.stickers.iter().filter(|e| !e.is_builtin).count();
    if non_builtin_count <= max_reg_num {
        return 0;
    }

    let excess = non_builtin_count - max_reg_num;

    // 收集非内置表情索引，按 (query_count, last_used_at) 升序排序
    let mut candidates: Vec<usize> = store.stickers.iter()
        .enumerate()
        .filter(|(_, e)| !e.is_builtin)
        .map(|(i, _)| i)
        .collect();

    candidates.sort_by(|&a, &b| {
        let entry_a = &store.stickers[a];
        let entry_b = &store.stickers[b];
        entry_a.query_count.cmp(&entry_b.query_count)
            .then(entry_a.last_used_at.cmp(&entry_b.last_used_at))
    });

    // 收集待移除的哈希
    let to_remove: std::collections::HashSet<String> = candidates.iter()
        .take(excess)
        .map(|&i| store.stickers[i].hash.clone())
        .collect();

    let mut evicted = 0;
    store.stickers.retain(|e| {
        if to_remove.contains(&e.hash) {
            let full_path = data_dir.join(&e.path);
            if full_path.exists() {
                std::fs::remove_file(&full_path).ok();
            }
            evicted += 1;
            false
        } else {
            true
        }
    });

    if evicted > 0 {
        info!(evicted, max_reg_num, "do_replace: eviction completed");
        save_store(&store);
    }
    evicted
}
