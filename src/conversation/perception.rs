//! CQ 码归一：把 QQ 富文本还原成她读得懂的话
//!
//! 群里大量消息不是纯文本：`/今日运势`、`/今日猪猪` 这类机器人回复裹在
//! `[CQ:markdown,…]` 里，`@` 是 HTML 转义的 `&#91;@名字&#93;` 加一个
//! `mqqapi://markdown/mention?at_tinyid=数字` 链接，还夹着图片链接与代码块。
//!
//! 只有 `[CQ:image,…]` 被剥掉过，其余原样进了她的 prompt：实测这类消息
//! **平均 727 字符、最长 1242 字符**，全是 markup。她看到的不是"谁 @ 了谁"，
//! 而是一坨转义字符与 URL——日志里"你@的是谁啊 看半天没看懂"、
//! "@的人都不认识"就是这么来的。
//!
//! 这里做三件事：
//! - **解出 @ 的对象**：`at_tinyid` / `[CQ:at,qq=]` 都还原成 `@名字`
//!   （名字由调用方提供，见 [`crate::conversation::mention`]）
//! - **解码转义**：`&#91;` → `[`、`&amp;` → `&` 等
//! - **剥掉 markup 噪声**：图片/视频/转发码、api 链接、代码块、markdown 标记

/// 归一后的感知文本与它的字符数上限
///
/// markdown 消息可以很长，但她的感知不需要全文：留一个上限，
/// 避免几十条长消息把 prompt 撑爆。
const MAX_PERCEIVED_CHARS: usize = 220;

/// 解码 QQ 富文本里常见的 HTML 实体
///
/// 只处理会被转义进消息正文的那几个（方括号与 & 是 markdown 提及的定界符，
/// 所以出现频率最高）。数字实体一并支持，因为实体会以 `&#91;` 形式出现。
pub fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let tail = &rest[pos..];
        let Some(semi) = tail.find(';') else {
            // 没有分号：当成普通 & 字符
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let entity = &tail[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            other => other
                .strip_prefix('#')
                .and_then(|n| {
                    if let Some(hex) = n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16).ok()
                    } else {
                        n.parse::<u32>().ok()
                    }
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &tail[semi + 1..];
            }
            None => {
                // 不认识的实体：原样保留，别把用户内容吃掉
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// 从 `mqqapi://markdown/mention?...&at_tinyid=123` 里取出被提及的 QQ 号
fn mention_tinyid(url: &str) -> Option<u64> {
    let after = url.split("at_tinyid=").nth(1)?;
    let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// 提及目标的呈现文本
///
/// 认不出的对象标注"你不认识这个人"：让她知道这是在叫一个陌生人，
/// 而不是留一串号码让她自己猜成某个熟人——"把所有人都当成豆"就是那么来的。
fn mention_label(qq: Option<u64>, lookup: &dyn Fn(u64) -> Option<String>) -> String {
    match qq {
        Some(qq) => match lookup(qq) {
            Some(name) if !name.trim().is_empty() => format!("@{}", name.trim()),
            _ => format!("@QQ{qq}（你不认识这个人）"),
        },
        None => "@某人".to_string(),
    }
}

/// 把两种提及形态统一换成 `@名字`
///
/// - `mqqapi://markdown/mention` 的 `at_tinyid` 是**权威** QQ 号：
///   比同一条消息里的显示名可靠（显示名可能带后缀、表情、被截断）
/// - 普通 `[CQ:at,qq=N]`（干净客户端发来的纯文本 @）
fn resolve_mentions(content: &str, lookup: &dyn Fn(u64) -> Option<String>) -> String {
    resolve_cq_at(&resolve_markdown_mentions(content, lookup), lookup)
}

/// 把 `[CQ:at,qq=N]` 换成 `@名字`
fn resolve_cq_at(text: &str, lookup: &dyn Fn(u64) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[CQ:at,qq=") {
        out.push_str(&rest[..start]);
        let after = &rest[start + "[CQ:at,qq=".len()..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        let Some(end) = after.find(']') else {
            out.push_str(&rest[start..]);
            return out;
        };
        out.push_str(&mention_label(digits.parse().ok(), lookup));
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// 把一段 markdown 正文里的"提及"替换成 `@名字`
fn resolve_markdown_mentions(content: &str, lookup: &dyn Fn(u64) -> Option<String>) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(pos) = rest.find("(mqqapi://markdown/mention") {
        // 这个链接是紧跟在 `[显示名]` 后面的，把前面的方括号块一起去掉
        if let Some(bracket_start) = out.rfind('[') {
            out.truncate(bracket_start);
        }
        let Some(close) = rest[pos..].find(')') else {
            rest = &rest[pos + 1..];
            continue;
        };
        let url = &rest[pos + 1..pos + close];
        out.push_str(&mention_label(mention_tinyid(url), lookup));
        rest = &rest[pos + close + 1..];
    }
    out.push_str(rest);
    out
}

/// 从 `[CQ:markdown,content=…]` 里取出 content 字段的值
///
/// CQ 参数之间用逗号分隔，但 content 本身可以含逗号，所以按首个逗号
/// 定位起点、用**最后一个** `]` 之前的内容作终点（这里拿到的已经是
/// 剥掉外层方括号的整段）。
fn markdown_content(body: &str) -> Option<&str> {
    let start = body.find("content=")? + "content=".len();
    Some(&body[start..])
}

/// 剥掉非文本 CQ 码，但把 markdown 的 content 取出来继续处理
///
/// `[CQ:image,…]` → `[图片]`（保留"这里有张图"这个事实，视觉描述由别处补）；
/// `[CQ:markdown,content=…]` → 只保留 content，其余参数丢掉；
/// 其它（video/file/forward/json…）连同参数一起丢掉。
///
/// 只管"外壳"，人名解析在后面的 [`resolve_mentions`] 里做。
fn strip_non_text_cq(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[CQ:") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let Some(end) = after.find(']') else {
            // 没有闭合括号：整段丢弃，避免把半截 CQ 码当正文
            return out;
        };
        let body = &after["[CQ:".len()..end];
        if after.starts_with("[CQ:image,") {
            out.push_str("[图片]");
        } else if let Some(content) = markdown_content(body) {
            // markdown 里可能还有别的 CQ 码（图片等），递归清一遍
            out.push_str(&strip_non_text_cq(content));
        }
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// 清掉 markdown 装饰与残留链接
fn strip_markup(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.lines() {
        let trimmed = line.trim();
        // 代码块围栏与引用标记
        if trimmed.starts_with("```") {
            continue;
        }
        let trimmed = trimmed.trim_start_matches(['>', '#']).trim_start();
        // 行内 api/图片链接：`[文字](url)` → 文字；裸 URL 直接丢
        let without_links = remove_markdown_links(trimmed);
        let cleaned = without_links.replace("***", "").replace("**", "");
        if !cleaned.trim().is_empty() {
            out.push_str(cleaned.trim());
            out.push('\n');
        }
    }
    out.trim().to_string()
}

/// `[文字](url)` → `文字`；带图片语义的整块换成 `[图片]`
fn remove_markdown_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        let Some(close_bracket) = after.find(']') else {
            out.push_str(after);
            return out;
        };
        let label = &after[1..close_bracket];
        let after_label = &after[close_bracket + 1..];
        if let Some(rest_after_url) = after_label.strip_prefix('(')
            && let Some(url_end) = rest_after_url.find(')')
        {
            let url = &rest_after_url[..url_end];
            if label.starts_with("img") || url.contains("ugcimg") || url.contains("multimedia") {
                out.push_str("[图片]");
            } else if url.starts_with("mqqapi://") {
                // 内联命令按钮（"不需要@就可以玩…的方法"之类）：纯噪声
            } else {
                out.push_str(label);
            }
            rest = &rest_after_url[url_end + 1..];
            continue;
        }
        // 不是链接：保留方括号内容，继续往后找
        out.push('[');
        rest = &after[1..];
    }
    out.push_str(rest);
    out
}

/// 把一条原始群消息归一成她能读的文本
///
/// `lookup` 把 QQ 号翻成她认得的名字（`None` 表示她不认识这个人）。
///
/// 顺序很重要：**先解提及，再剥 CQ 外壳**。反过来的话
/// `[CQ:at,qq=N]` 会被当成"非文本 CQ 码"整块丢掉，她就再也看不到
/// 谁 @ 了谁——那正是"@的人都不认识"的成因。
pub fn normalize(raw: &str, lookup: &dyn Fn(u64) -> Option<String>) -> String {
    let decoded = decode_entities(raw);
    let with_mentions = resolve_mentions(&decoded, lookup);
    let unwrapped = strip_non_text_cq(&with_mentions);
    let plain = strip_markup(&unwrapped);
    // 压掉多余空白（markdown 换行很多，但一句话不需要那么多行）
    let collapsed: String = plain
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    truncate_chars(&collapsed, MAX_PERCEIVED_CHARS)
}

/// 按字符（不是字节）截断，中文不会被切坏
fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup<'a>(pairs: &'a [(u64, &'a str)]) -> impl Fn(u64) -> Option<String> + 'a {
        move |qq| {
            pairs
                .iter()
                .find(|(id, _)| *id == qq)
                .map(|(_, n)| (*n).to_string())
        }
    }

    /// 真实日志里的原文（2026-09-18 08:49:17，`/今日猪猪` 的一次回复）
    const REAL_MARKDOWN: &str = concat!(
        "[CQ:markdown,content=&#91;&#93;(%7B%22version%22%3A2%7D)\n",
        "&#91;@比比哈勃体积内超级无敌最爱喝豆奶更爱喵&#93;",
        "(mqqapi://markdown/mention?at_type=1&at_tinyid=2270348091)\n",
        "🎉 抓到一只新猪猪啦！\n",
        "!&#91;图片 #120px #120px&#93;(https://qqbot.ugcimg.cn/102076836/x.jpg)\n",
        "##### 【猪卖弱】\n",
        "我好菜啊\n",
        "> 你逢人就说自己菜，但大家都知道你是猪圈里段位最高的那个。\n",
        "> &#91;🐝不需要@就可以玩幽幽子的方法&#93;(mqqapi://aio/inlinecmd?command=%2F)\n",
        "]@比比哈勃体积内超级无敌最爱喝豆奶更爱喵 🎉 抓到一只新猪猪啦！"
    );

    #[test]
    fn decodes_the_entities_qq_escapes() {
        assert_eq!(decode_entities("&#91;@甲&#93;"), "[@甲]");
        assert_eq!(decode_entities("a&amp;b"), "a&b");
        assert_eq!(decode_entities("&#x41;"), "A");
        // 未知实体与裸 & 都要原样留住
        assert_eq!(decode_entities("100&unknown;"), "100&unknown;");
        assert_eq!(decode_entities("A & B"), "A & B");
    }

    #[test]
    fn a_real_pig_message_becomes_readable_and_names_the_person() {
        let names = [(2270348091u64, "豆奶")];
        let out = normalize(REAL_MARKDOWN, &lookup(&names));

        assert!(out.contains("@豆奶"), "要指出她 @ 的是谁：{out}");
        assert!(out.contains("抓到一只新猪猪"), "正文要留下：{out}");
        // 原始 markup 一个都不该剩
        for noise in [
            "CQ:markdown",
            "mqqapi",
            "at_tinyid",
            "&#91;",
            "ugcimg",
            "```",
        ] {
            assert!(!out.contains(noise), "残留了 {noise}：{out}");
        }
    }

    #[test]
    fn unknown_mention_target_is_flagged_as_unknown() {
        let out = normalize(REAL_MARKDOWN, &|_| None);
        assert!(out.contains("2270348091"), "{out}");
        assert!(out.contains("你不认识"), "认不出就该明说：{out}");
    }

    #[test]
    fn long_markdown_is_capped() {
        let long = format!("[CQ:markdown,content={}]", "唠".repeat(800));
        let out = normalize(&long, &|_| None);
        assert!(
            out.chars().count() <= MAX_PERCEIVED_CHARS + 1,
            "太长要截断，实际 {} 字符：{out}",
            out.chars().count()
        );
        assert!(out.ends_with('…'), "截断要留痕迹：{out}");
    }

    #[test]
    fn plain_text_passes_through_untouched() {
        let out = normalize("今晚吃火锅吗", &|_| None);
        assert_eq!(out, "今晚吃火锅吗");
    }

    #[test]
    fn images_leave_a_placeholder_not_a_url() {
        let out = normalize("看这个[CQ:image,file=a.jpg,url=https://x/y.jpg]", &|_| None);
        assert_eq!(out, "看这个[图片]");
    }

    #[test]
    fn other_cq_codes_are_dropped_entirely() {
        let out = normalize(
            "[CQ:video,file=x,url=https://example.com/a?rkey=secret]真的",
            &|_| None,
        );
        assert_eq!(out, "真的");
        assert!(!out.contains("rkey"));
    }

    #[test]
    fn inline_command_buttons_do_not_reach_her() {
        let raw = "抽到了：\n> &#91;🐝不需要@就可以玩幽幽子的方法&#93;(mqqapi://aio/inlinecmd?command=%2F)";
        let out = normalize(raw, &|_| None);
        assert!(!out.contains("mqqapi"), "{out}");
        assert!(!out.contains("inlinecmd"), "{out}");
        assert!(out.contains("抽到了"), "{out}");
    }

    #[test]
    fn markdown_decoration_is_removed() {
        let out = normalize(
            "***✨您的今日运势为：***\n```html\n<吉祥话>\n```",
            &|_| None,
        );
        assert!(!out.contains("***"), "{out}");
        assert!(!out.contains("```"), "{out}");
        assert!(!out.contains("html"), "{out}");
        assert!(out.contains("今日运势"), "{out}");
    }

    #[test]
    fn mention_url_without_tinyid_still_hides_the_url() {
        let raw = "[这个人的主页](mqqapi://markdown/mention?at_type=1) 你好";
        let out = normalize(raw, &|_| None);
        assert!(!out.contains("mqqapi"), "{out}");
        assert!(out.contains("你好"), "{out}");
    }

    #[test]
    fn multiple_mentions_in_one_message_all_resolve() {
        let names = [(1u64, "甲"), (2u64, "乙")];
        let raw = "&#91;@甲&#93;(mqqapi://markdown/mention?at_type=1&at_tinyid=1)和&#91;@乙&#93;(mqqapi://markdown/mention?at_type=1&at_tinyid=2)都来";
        let out = normalize(raw, &lookup(&names));
        assert!(out.contains("@甲"), "{out}");
        assert!(out.contains("@乙"), "{out}");
        assert!(out.contains("都来"), "{out}");
    }

    #[test]
    fn ordinary_brackets_are_not_eaten() {
        let out = normalize("我买了[书](实体) 和 [笔]", &|_| None);
        assert!(out.contains("[笔]") || out.contains("笔"), "{out}");
        assert!(out.contains("书"), "{out}");
    }

    // ── 真实日志里的两段序列（2026-09-18 09:43~09:45） ──────────
    //
    // 这一段是用户报的核心症状：有人明确 @ 了机器人自己，她却在说
    // "@的人都不认识"。除了"解析不出来"，还有一个必须成立的事实：
    // **她自己得知道那个号是她**。

    /// 机器人自己的 QQ（日志里的自号，与数据目录 512166443 一致）
    const SELF_QQ: u64 = 512166443;

    /// 她认得的几个人 + 她自己
    fn roster(qq: u64) -> Option<String> {
        match qq {
            SELF_QQ => Some("洛玖".to_string()),
            2557657882 => Some("洛洛".to_string()),
            3044702204 => Some("ebichuuu".to_string()),
            _ => None,
        }
    }

    #[test]
    fn being_mentioned_by_qq_shows_her_own_name_not_a_stranger() {
        // "[CQ:at,qq=512166443] 我喜欢你" —— 这个号就是她自己
        //
        // 归一层的职责：查得到名字、不露号码、不标成陌生人。
        // "她自己知道那个号是她"由 self_qq 决定（见 config::check_self_qq）：
        // 只有 self_qq 配对时，这个查找才会返回她的名字而不是 None。
        let out = normalize(&format!("[CQ:at,qq={SELF_QQ}] 我喜欢你"), &|qq| {
            roster(qq)
        });
        assert_eq!(out, "@洛玖 我喜欢你");
        assert!(
            !out.contains("不认识"),
            "被 @ 的是她自己，绝不能标成不认识：{out}"
        );
        assert!(!out.contains(&SELF_QQ.to_string()), "不该露号码：{out}");
    }

    #[test]
    fn wrong_self_qq_is_what_turns_her_into_a_stranger_to_herself() {
        // 复现线上真实故障：self_qq 配成了别的号（日志里是 2723624307，
        // 而她是 512166443）。按那个号去查名字查不到，她就被自己的 @
        // 标注成"你不认识这个人"——正是日志里 09:44:45 那句话的成因。
        const WRONG_SELF_QQ: u64 = 2723624307;
        let lookup_by_config = |qq: u64| {
            if qq == WRONG_SELF_QQ {
                Some("洛玖".to_string())
            } else {
                None
            }
        };
        let out = normalize(&format!("[CQ:at,qq={SELF_QQ}] 我喜欢你"), &lookup_by_config);
        assert!(out.contains("不认识"), "self_qq 配错时她认不出自己：{out}");
    }

    #[test]
    fn mention_of_someone_else_names_that_person() {
        // 09:43:38 "[CQ:reply,id=…][CQ:at,qq=3044702204] 涨价我就跑去用glm5.3f"
        // 她当时说"你@的是谁啊 看半天没看懂"——@ 的对象就在消息里
        let out = normalize(
            "[CQ:reply,id=459840962][CQ:at,qq=3044702204] 涨价我就跑去用glm5.3f",
            &|qq| roster(qq),
        );
        assert!(out.contains("@ebichuuu"), "要指出她 @ 的是谁：{out}");
        assert!(out.contains("涨价"), "正文要留下：{out}");
        assert!(!out.contains("CQ:"), "不该留 CQ 码：{out}");
        assert!(!out.contains("3044702204"), "不该露号码：{out}");
    }

    #[test]
    fn a_stranger_mention_is_marked_unknown_but_still_names_nothing_wrong() {
        // 号不在她认识的人里：明说认不出，而不是留号码让她猜成熟人
        let out = normalize("[CQ:at,qq=999999999] 你好", &|qq| roster(qq));
        assert!(out.contains("不认识"), "{out}");
        assert!(out.contains("999999999"), "要带上号码好排查：{out}");
    }

    #[test]
    fn cleaning_a_message_does_not_invent_an_identity() {
        // 认不出的号 + 她自己同时在一条里：只有她自己该被认出来
        let out = normalize(
            &format!("[CQ:at,qq=999999999][CQ:at,qq={SELF_QQ}] 你俩"),
            &|qq| roster(qq),
        );
        assert!(out.contains("@洛玖"), "{out}");
        assert!(out.contains("不认识"), "{out}");
    }
}
