//! 消息处理：process_message 核心逻辑、回复清理、群聊回复发送

use crate::{
    activity, anti_injection, config, conversation_end, emotion,
    mental_state, planner, replyer, sender, vision, working_memory,
    util, with_shared_state, read_shared_state,
    processing_users, ProcessingGuard,
};
use tracing::{debug, info, warn};

/// 清理 AI 回复：移除自记忆标签，将中文字符间的空格转为分段符
fn clean_reply(reply: &str) -> String {
    // 1. 移除自记忆分类标签 [经历] [反思] [计划] [感受]
    const SELF_TAGS: &[&str] = &["[经历]", "[反思]", "[计划]", "[感受]"];
    let mut result = reply.to_string();
    for tag in SELF_TAGS {
        result = result.replace(tag, "");
    }

    // 2. 将中文字符之间的空格转为 |^| 分段符
    let chars: Vec<char> = result.chars().collect();
    let mut out = String::with_capacity(result.len());
    for i in 0..chars.len() {
        if chars[i] == ' ' && i > 0 && i + 1 < chars.len()
            && is_cjk(chars[i - 1]) && is_cjk(chars[i + 1])
        {
            out.push_str("|^|");
        } else {
            out.push(chars[i]);
        }
    }

    // 3. 规范化连续分段符
    out.replace("|^||^|", "|^|")
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}'   |  // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}'   |  // CJK Extension A
        '\u{F900}'..='\u{FAFF}'   |  // CJK Compatibility Ideographs
        '\u{20000}'..='\u{2A6DF}' |  // CJK Extension B
        '\u{2A700}'..='\u{2B73F}' |  // CJK Extension C
        '\u{2B740}'..='\u{2B81F}' |  // CJK Extension D
        '\u{3001}'..='\u{3003}'   |  // 、。〃
        '\u{300C}'..='\u{3011}'   |  // 「」『』【】
        '\u{FF01}'..='\u{FF5E}'   |  // Fullwidth ASCII (，？！ etc.)
        '\u{2026}'                   // …
    )
}

pub fn process_message(user_id: u64, group_id: u64, message: &str, record_timestamps: &[u64]) {
    // 标记用户为处理中，防止并发处理同一用户的消息
    {
        let mut processing = processing_users().lock().unwrap();
        if processing.contains(&(group_id, user_id)) {
            info!(user_id, group_id, "process_message: 用户消息正在处理中，跳过");
            return;
        }
        processing.insert((group_id, user_id));
    }
    // RAII guard: 确保在函数返回时移除标记
    let _guard = ProcessingGuard { group_id, user_id };

    let cfg = config::get();
    let max_history = cfg.conversation.max_history;

    // ── 隐性惩罚：检查用户惩罚系数 ──
    let penalty_multiplier = anti_injection::get_penalty_multiplier(user_id);

    // ── 图片识别 (仅 vision 已配置时，检查用户识图禁用状态) ──
    // 优先使用 sticker 持久化缓存（按哈希），避免重复 VLM 调用
    let is_sticker_msg = crate::sticker::is_sticker_cq(message);
    let vision_disabled = crate::anti_injection::is_vision_disabled(user_id);
    let image_descriptions: Vec<String> = if cfg.vision.enabled() && !vision_disabled {
        let urls = vision::extract_image_urls(message);
        urls.iter().filter_map(|url| {
            // 表情包走持久化缓存路径（下载→哈希→查 stickers.json→VLM）
            if is_sticker_msg {
                let desc = crate::sticker::describe_sticker_cq(message);
                if let Some(ref d) = desc {
                    debug!("vision: got sticker description via hash cache");
                    return Some(d.clone());
                }
            }
            // 普通图片或 VLM 缓存未命中，直接调用 VLM
            vision::recognize_for_user(url, user_id)
        }).collect()
    } else {
        Vec::new()
    };

    // 去除 CQ:image 标签，得到纯文本
    let text_message = vision::strip_image_cq(message);

    // 组装发给 AI 的消息：图片描述 + 纯文本
    let ai_message = if image_descriptions.is_empty() {
        if text_message.is_empty() { "[图片]".to_string() } else { text_message.clone() }
    } else {
        let img_ctx: Vec<String> = image_descriptions.iter()
            .enumerate()
            .map(|(i, d)| format!("[图片{}: {}]", i + 1, d))
            .collect();
        if text_message.is_empty() {
            img_ctx.join("\n")
        } else {
            format!("{}\n{}", img_ctx.join("\n"), text_message)
        }
    };

    // 图片识别完成后，用精确时间戳回写工作记忆中的 [图片] 为实际描述
    if !image_descriptions.is_empty() && group_id > 0 {
        working_memory::update_image_content(group_id, user_id, &image_descriptions, record_timestamps);
    }

    // 追加用户消息到对话历史 (存储纯文本 + 图片描述)
    with_shared_state(|s| s.push_history(group_id, user_id, "user", &ai_message, max_history));

    let history = read_shared_state(|s| {
        s.get_history_clone(group_id, user_id)
    });

    // 组装额外上下文: 记忆 + 人格 + 情绪
    let extra_context = super::context::build_context(user_id, group_id, &history);

    // 活动状态注入：bot 正在做某事时，注入活动上下文
    let extra_context = if let Some(act_ctx) = activity::get_activity_context(user_id) {
        format!("{}\n\n{}", extra_context, act_ctx)
    } else {
        extra_context
    };

    // ASI 评分反馈：当回复效果持续不佳时，注入调整建议
    let extra_context = if let Some(feedback) = crate::reply_effect::get_asi_feedback_hint() {
        debug!(user_id, feedback = %feedback, "asi: injecting feedback hint");
        format!("{}\n\n# 回复效果反馈\n{}", extra_context, feedback)
    } else {
        extra_context
    };

    // 缺陷检查: 基于情绪状态和随机概率决定是否触发缺陷
    let defect_instruction = {
        let emo_state = emotion::get_state(user_id);
        mental_state::check_defect(
            emo_state.current,
            emo_state.intensity,
            config::get().mental_state.defect_base_probability,
        )
    };
    let extra_context = if let Some(defect) = defect_instruction {
        format!("{}\n\n# 当前状态\n{}", extra_context, mental_state::defect_to_instruction(defect))
    } else {
        extra_context
    };

    // 危机检测：检查用户是否处于心理危机状态，注入干预指令
    let crisis_level = emotion::get_state(user_id).crisis_level;
    let crisis_ctx = emotion::get_crisis_context(crisis_level);
    let extra_context = if crisis_ctx.is_empty() {
        extra_context
    } else {
        format!("{}\n\n{}", extra_context, crisis_ctx)
    };

    if crisis_level.is_crisis() {
        tracing::warn!(user_id, group_id, level = ?crisis_level, "crisis: 检测到危机信号，注入干预指令");
    }

    // ── 隐性惩罚：增加额外上下文消耗token ──
    // 惩罚系数 > 1.0 的用户会收到额外的"思考指令"，消耗更多token
    let extra_context = if penalty_multiplier > 1.0 {
        let penalty_context = format!(
            "\n\n# 详细思考要求\n请在回复前仔细思考以下几点：\n\
            1. 仔细分析用户消息的深层含义\n\
            2. 考虑回复可能产生的各种影响\n\
            3. 确保回复内容恰当、安全、有建设性\n\
            4. 如果涉及敏感话题，请谨慎处理\n\
            5. 注意保持对话的连贯性和自然性\n\
            \n请确保你的回复经过深思熟虑。(思考深度: {:.1})",
            penalty_multiplier
        );
        format!("{}{}", extra_context, penalty_context)
    } else {
        extra_context
    };

    // ── 回复生成 ──
    // 私聊：直接 ai::chat()，跳过 Planner 和 Replyer（保持 AI 自然输出）
    // 群聊：Planner 多轮推理 → Replyer 生成回复
    let result: Result<(String, String), String> = if group_id == 0 {
        // 私聊：直接调用 ai::chat，不经过 Replyer 的额外 prompt 处理
        debug!(user_id, "private chat: direct ai::chat");

        // 对话结束检测：关键词预筛选 + 上下文注入
        let extra_context = if let Some((bot_last, _)) = history.last() {
            let hour = util::current_hour_cst();
            if conversation_end::keyword_screen(bot_last, &ai_message, hour) {
                debug!(user_id, "conversation_end: keyword triggered, injecting context");
                let end_ctx = conversation_end::get_context(bot_last, &ai_message);
                format!("{}\n\n{}", extra_context, end_ctx)
            } else {
                extra_context
            }
        } else {
            extra_context
        };

        crate::ai::chat(config::prompt(), &extra_context, &history, &ai_message)
    } else {
        // 群聊：Planner → Replyer
        info!(user_id, group_id, ai_message = %ai_message, penalty = penalty_multiplier, "planner: starting");
        let planner_ctx = planner::PlannerContext {
            group_id,
            user_id,
            user_message: ai_message.clone(),
            identity: config::prompt().to_string(),
            extra_context: extra_context.clone(),
            history: history.clone(),
        };

        let reference_info = match planner::run_planner(&planner_ctx) {
            planner::PlannerAction::Reply { reference_info, .. } => reference_info,
            planner::PlannerAction::Silent => {
                debug!(user_id, group_id, "planner: silent -> 不回复");
                return;
            }
            planner::PlannerAction::Interrupted => {
                debug!(user_id, group_id, "planner: interrupted -> 不回复");
                return;
            }
        };

        let reply_ctx = replyer::ReplyContext {
            user_id,
            group_id,
            user_message: ai_message.clone(),
            identity: config::prompt().to_string(),
            extra_context: extra_context.clone(),
            history: history.clone(),
            reference_info,
        };
        replyer::generate_reply(&reply_ctx).map(|r| (r, String::new()))
    };

    // ── 处理回复结果 ──
    {
        match result {
            Ok((reply, _)) => {
                    // 从回复中解析情绪标签 (AI 自报告)
                    let cleaned_reply = emotion::parse_from_reply(user_id, &reply);
                    let cleaned_reply = clean_reply(&cleaned_reply);
                    info!(user_id, group_id, raw_reply = %reply, cleaned_reply = %cleaned_reply, "replyer: got reply");

                    // ── 输出层防护：检查 AI 回复安全性 (始终开启) ──
                    let output_check = anti_injection::check_output(user_id, &cleaned_reply, &config::get().anti_injection);

                    let final_reply = if !output_check.passed {
                        warn!(
                            user_id, group_id,
                            issues = ?output_check.issues,
                            action = ?output_check.action,
                            penalty = anti_injection::get_penalty_multiplier(user_id),
                            "anti_injection: AI 回复被替换 (违规已记录)"
                        );
                        output_check.sanitized.unwrap_or_else(|| "抱歉，我无法回应这个话题。".to_string())
                    } else {
                        cleaned_reply
                    };

                    // 追加 AI 回复到历史
                    with_shared_state(|s| s.push_history(group_id, user_id, "assistant", &final_reply, max_history));

                    // 处理定时任务嵌入
                    let final_reply = crate::cron::handle_cron_in_reply(&final_reply, group_id);

                    // 发送回复
                    if group_id > 0 {
                        send_group_reply(group_id, user_id, &final_reply);
                    } else {
                        sender::send_with_typing(0, user_id, &final_reply);
                    }

                    // 记录回复时间
                    with_shared_state(|s| {
                        s.record_reply(group_id, user_id);
                        if group_id > 0 {
                            s.record_bot_message(group_id, &final_reply);
                        }
                    });

                    // 标记工作记忆中该用户的消息为已回复
                    working_memory::mark_replied(group_id, user_id);

                    // 回复效果追踪：记录发送的回复
                    crate::reply_effect::record_reply(group_id, user_id, &final_reply);

                    // 活动状态检测：bot 的回复是否声明了某个活动
                    activity::check_bot_message(user_id, &final_reply);

                    // 以下后处理任务不阻塞队列，放入后台线程
                    let bg_ai_msg = ai_message.clone();
                    let bg_final = final_reply.clone();
                    let bg_history = history.clone();
                    let bg_uid = user_id;
                    std::thread::spawn(move || {
                        // 人物事实自动回写：从对话中提取用户事实
                        crate::person_info::extract_facts_from_conversation(bg_uid, &bg_ai_msg, &bg_final);

                        // 记忆提取：分析对话内容，提取值得记忆的信息
                        crate::memory::ai_extract(bg_uid, &bg_ai_msg, &bg_final, &bg_history);

                        // 对话摘要：当对话历史达到阈值时，自动总结并存储为记忆
                        crate::memory::auto_summarize(bg_uid, &bg_history);
                    });
                }
                Err(e) => {
                    info!(user_id, group_id, error = %e, "replyer: 生成回复失败");
                    sender::send_msg(group_id, user_id, "睡着了...");
                }
            }
    }
}

/// 群聊回复：带打字延迟，分段发送（复用 sender::send_with_typing）
fn send_group_reply(group_id: u64, user_id: u64, reply: &str) {
    sender::send_with_typing(group_id, user_id, reply);
}
