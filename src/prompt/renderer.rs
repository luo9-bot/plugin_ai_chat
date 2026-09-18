use std::collections::HashMap;

/// Prompt 渲染器：负责占位符替换
///
/// 占位符格式：`{key}`，缺失的占位符保留原样。
///
/// 这里曾经还有一套"严格渲染器"（`render` + `RenderOptions` +
/// `find_missing_placeholders*`），它从未有过调用点，而且用
/// `regex::Regex::new(..).unwrap()` 在库里持有生产代码的 panic 风险——
/// 已删除。需要诊断缺失占位符时应重新设计，而不是留着不可达的第二实现。
pub(crate) struct PromptRenderer;

impl PromptRenderer {
    /// 渲染模板，替换所有 {key} 占位符（缺失的保留原样）
    pub fn render_simple(template: &str, vars: &HashMap<&str, &str>) -> String {
        let mut result = template.to_string();
        for (key, value) in vars {
            result = result.replace(&format!("{{{}}}", key), value);
        }
        result
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
    fn missing_placeholders_are_left_untouched() {
        let vars = HashMap::new();
        assert_eq!(
            PromptRenderer::render_simple("我叫{name}", &vars),
            "我叫{name}"
        );
    }
}
