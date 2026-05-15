use std::collections::HashMap;

/// Prompt 渲染器：负责占位符替换与缺失校验
///
/// 剥离当前 PromptManager 中混杂的渲染逻辑，单独聚焦渲染职责。
/// • 占位符格式：`{key}`
/// • 缺失占位符默认保留原样，但可以开启严格模式报错
pub struct PromptRenderer;

#[derive(Debug, Clone)]
pub struct RenderOptions {
    /// 严格模式：缺失占位符时报错而非静默保留
    pub strict: bool,
    /// 自定义渲染前/后处理
    pub pre_process: Option<fn(&str) -> String>,
    pub post_process: Option<fn(&str) -> String>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            strict: false,
            pre_process: None,
            post_process: None,
        }
    }
}

impl PromptRenderer {
    /// 渲染模板，替换所有 {key} 占位符
    ///
    /// # 参数
    /// * `template` - 模板文本
    /// * `vars` - 占位符映射表
    /// * `options` - 渲染选项
    ///
    /// # 返回
    /// 渲染后的文本。若 strict 模式开启且存在缺失占位符，返回 Err。
    pub fn render(
        template: &str,
        vars: &HashMap<&str, &str>,
        options: &RenderOptions,
    ) -> Result<String, Vec<String>> {
        let processed = if let Some(pre) = options.pre_process {
            pre(template)
        } else {
            template.to_string()
        };

        // strict 模式：检测模板中是否有未提供的占位符
        if options.strict {
            let missing = Self::find_missing_placeholders_in_template(&processed, vars);
            if !missing.is_empty() {
                return Err(missing);
            }
        }

        let mut result = processed;
        for (key, value) in vars {
            result = result.replace(&format!("{{{}}}", key), value);
        }

        let result = if let Some(post) = options.post_process {
            post(&result)
        } else {
            result
        };

        Ok(result)
    }

    /// 简易渲染（无配置，缺失占位符保留原样）
    pub fn render_simple(template: &str, vars: &HashMap<&str, &str>) -> String {
        let mut result = template.to_string();
        for (key, value) in vars {
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result
    }

    /// 从渲染结果中提取所有未替换的占位符（用于诊断）
    pub fn find_missing_placeholders(template: &str) -> Vec<String> {
        let mut found = Vec::new();
        let re = regex::Regex::new(r"\{([a-zA-Z_][a-zA-Z0-9_]*)}").unwrap();
        for cap in re.captures_iter(template) {
            if let Some(m) = cap.get(1) {
                found.push(m.as_str().to_string());
            }
        }
        found
    }

    /// strict 模式下：在模板中查找所有占位符，检测 vars 中是否都提供了
    fn find_missing_placeholders_in_template(template: &str, vars: &HashMap<&str, &str>) -> Vec<String> {
        let mut missing = Vec::new();
        let re = regex::Regex::new(r"\{([a-zA-Z_][a-zA-Z0-9_]*)}").unwrap();
        for cap in re.captures_iter(template) {
            if let Some(m) = cap.get(1) {
                let key = m.as_str();
                if !vars.contains_key(key) {
                    missing.push(key.to_string());
                }
            }
        }
        missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_simple() {
        let mut vars = HashMap::new();
        vars.insert("name", "麦麦");
        vars.insert("emotion", "开心");
        let result = PromptRenderer::render_simple("我叫{name}，今天很{emotion}", &vars);
        assert_eq!(result, "我叫麦麦，今天很开心");
    }

    #[test]
    fn test_strict_missing() {
        let mut vars = HashMap::new();
        vars.insert("name", "麦麦");
        let result = PromptRenderer::render(
            "我叫{name}，今天{emotion}",
            &vars,
            &RenderOptions { strict: true, ..Default::default() },
        );
        assert!(result.is_err());
    }
}
