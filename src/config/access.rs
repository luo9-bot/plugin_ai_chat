use std::collections::HashSet;
use std::path::PathBuf;

use super::init::{CONFIG, PROMPT, DATA_DIR, DEFAULT_CONFIG_YAML};
use super::structs::Config;

pub fn data_dir() -> &'static PathBuf {
    DATA_DIR.get().expect("Config not initialized")
}

pub fn get() -> &'static Config {
    CONFIG.get().expect("Config not initialized")
}

pub fn prompt() -> &'static str {
    PROMPT.get().map(|s| s.as_str()).unwrap_or("")
}

/// 返回配置模板（带注释的 YAML）
pub fn config_template() -> &'static str {
    DEFAULT_CONFIG_YAML
}

/// 使用模板保存配置：保留注释和格式，只替换值
pub fn save_config_with_comments(config: &serde_json::Value) -> Result<String, String> {
    let template = DEFAULT_CONFIG_YAML;
    let new_obj = config.as_object().ok_or("config is not an object")?;

    let mut output = String::with_capacity(template.len() + 512);
    let mut indent_stack: Vec<&str> = Vec::new();
    let mut skip_list_items = false;
    let mut handled_keys: HashSet<&str> = HashSet::new();

    for line in template.lines() {
        let trimmed = line.trim();

        // 注释或空行：原样保留
        if trimmed.starts_with('#') || trimmed.is_empty() {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        // 计算当前缩进深度
        let indent = line.len() - line.trim_start().len();
        let depth = indent / 2;
        let indent_str = &line[..indent];

        // 更新缩进栈
        while indent_stack.len() > depth {
            indent_stack.pop();
        }

        // 列表项（- 开头）：如果父 key 的数组已被替换，跳过
        if trimmed.starts_with('-') && skip_list_items {
            continue;
        }

        // 遇到新的 key，重置跳过标记
        if trimmed.find(':').is_some() {
            skip_list_items = false;
        }

        // 解析 key: value
        if let Some(colon_pos) = trimmed.find(':') {
            let key = trimmed[..colon_pos].trim();
            let old_rest = &trimmed[colon_pos + 1..];
            let old_rest_trimmed = old_rest.trim();

            // 嵌套块（key: 后面没值）
            if old_rest_trimmed.is_empty() {
                // 检查新值是否是数组（需要替换后续列表项）
                let new_val = find_nested_value(new_obj, &indent_stack, key);
                if let Some(serde_json::Value::Array(arr)) = new_val {
                    output.push_str(line);
                    output.push('\n');
                    // 输出新数组的列表项
                    let item_indent = format!("{}  ", indent_str);
                    for item in arr {
                        let item_yaml = value_to_yaml_inline(item);
                        output.push_str(&format!("{}- {}\n", item_indent, item_yaml));
                    }
                    // 标记跳过后续的模板列表项
                    skip_list_items = true;
                    indent_stack.push(key);
                    continue;
                }
                if indent_stack.is_empty() {
                    handled_keys.insert(key);
                }
                indent_stack.push(key);
                output.push_str(line);
                output.push('\n');
                continue;
            }

            // 在 new_obj 中查找对应值
            let new_value = find_nested_value(new_obj, &indent_stack, key);

            if let Some(val) = new_value {
                // 检查是否是数组（需要输出为列表格式）
                if let serde_json::Value::Array(arr) = val {
                    let comment = extract_inline_comment(old_rest);
                    if comment.is_empty() {
                        output.push_str(&format!("{}{}:\n", indent_str, key));
                    } else {
                        output.push_str(&format!("{}{}: {}\n", indent_str, key, comment));
                    }
                    // 输出数组项为列表格式
                    let item_indent = format!("{}  ", indent_str);
                    for item in arr {
                        let item_yaml = value_to_yaml_inline(item);
                        output.push_str(&format!("{}- {}\n", item_indent, item_yaml));
                    }
                    // 标记跳过后续的模板列表项
                    skip_list_items = true;
                    if indent_stack.is_empty() {
                        handled_keys.insert(key);
                    }
                } else {
                    let new_yaml = value_to_yaml_inline(val);
                    let comment = extract_inline_comment(old_rest);
                    if comment.is_empty() {
                        output.push_str(&format!("{}{}: {}\n", indent_str, key, new_yaml));
                    } else {
                        output.push_str(&format!("{}{}: {} {}\n", indent_str, key, new_yaml, comment));
                    }
                    if indent_stack.is_empty() {
                        handled_keys.insert(key);
                    }
                }
            } else {
                output.push_str(line);
                output.push('\n');
            }
        } else {
            // 列表项或其他非 key: value 行
            output.push_str(line);
            output.push('\n');
        }
    }

    // 追加模板中没有的顶级 key（如 whitelist、blacklist 等）
    for (key, val) in new_obj {
        if !handled_keys.contains(key.as_str()) {
            let yaml_val = value_to_yaml_inline(val);
            output.push_str(&format!("{}: {}\n", key, yaml_val));
        }
    }

    Ok(output)
}

/// 在嵌套对象中查找值
fn find_nested_value<'a>(
    root: &'a serde_json::Map<String, serde_json::Value>,
    stack: &[&str],
    key: &str,
) -> Option<&'a serde_json::Value> {
    let mut current = root;
    for parent in stack {
        current = current.get(*parent)?.as_object()?;
    }
    current.get(key)
}

/// 将 JSON Value 转为 YAML 内联格式
fn value_to_yaml_inline(val: &serde_json::Value) -> String {
    match val {
        serde_json::Value::String(s) => {
            if s.contains(':') || s.contains('#') || s.contains('"') || s.starts_with(' ') {
                format!("\"{}\"", s.replace('"', "\\\""))
            } else {
                s.clone()
            }
        }
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(|v| value_to_yaml_inline(v)).collect();
            format!("[{}]", items.join(", "))
        }
        serde_json::Value::Object(map) => {
            // 按预定义顺序输出字段，未定义的排在最后按字母序
            let ordered = object_to_ordered_pairs(map);
            format!("{{{}}}", ordered.join(", "))
        }
    }
}

/// 已知结构的字段顺序（不在列表中的键按字母序追加到末尾）
fn field_order_hint(obj: &serde_json::Map<String, serde_json::Value>) -> Option<&'static [&'static str]> {
    // 配额时段
    if obj.contains_key("start_hour") && obj.contains_key("end_hour") && obj.contains_key("max_replies") {
        return Some(&["start_hour", "end_hour", "max_replies"]);
    }
    None
}

fn object_to_ordered_pairs(map: &serde_json::Map<String, serde_json::Value>) -> Vec<String> {
    let hint = field_order_hint(map);
    let mut pairs = Vec::with_capacity(map.len());
    if let Some(order) = hint {
        // 按预定义顺序输出
        for &key in order {
            if let Some(val) = map.get(key) {
                pairs.push(format!("{}: {}", key, value_to_yaml_inline(val)));
            }
        }
        // 追加不在预定义顺序中的键（字母序）
        for (k, v) in map {
            if !order.contains(&k.as_str()) {
                pairs.push(format!("{}: {}", k, value_to_yaml_inline(v)));
            }
        }
    } else {
        // 无特殊顺序要求，按字母序
        for (k, v) in map {
            pairs.push(format!("{}: {}", k, value_to_yaml_inline(v)));
        }
    }
    pairs
}

/// 提取行尾注释（如 "value  # 注释" -> "# 注释"）
fn extract_inline_comment(rest: &str) -> &str {
    // 在 "value" 之后找 "#"，但要排除引号内的 #
    let chars: Vec<char> = rest.trim().chars().collect();
    let mut in_quote = false;
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '"' => in_quote = !in_quote,
            '#' if !in_quote => {
                // 找到注释起始位置
                let trimmed = rest.trim();
                return &trimmed[i..];
            }
            _ => {}
        }
    }
    ""
}
