use std::collections::{BTreeSet, HashMap};
use crate::runtime::history::{ContextMessage, collect_tool_search_call_id_set};
use crate::runtime::tool_registry::ToolSpec;

/// Deferred Tool 运行时状态
///
/// 核心变化：从 TTL 升级为上下文证据链同步。
/// 工具可见性的唯一依据是"当前上下文中是否存在对应的 tool_search call + result"。
pub struct DeferredToolState {
    /// 所有 deferred 工具的规格（name -> spec）
    pub deferred_specs: HashMap<String, ToolSpec>,
    /// 当前会话中已发现的工具名
    pub discovered_tool_names: BTreeSet<String>,
}

impl DeferredToolState {
    pub fn new(deferred_specs: Vec<ToolSpec>) -> Self {
        let specs_map: HashMap<String, ToolSpec> = deferred_specs
            .into_iter()
            .map(|spec| (spec.name.clone(), spec))
            .collect();
        Self {
            deferred_specs: specs_map,
            discovered_tool_names: BTreeSet::new(),
        }
    }

    /// 根据当前选中的上下文同步已发现工具
    ///
    /// 只有能够在上下文中找到完整 tool_search call + tool_result 的工具才会保留可见。
    pub fn sync_with_context(&mut self, selected_history: &[ContextMessage]) {
        let search_call_ids = collect_tool_search_call_id_set(selected_history);
        let mut still_valid = BTreeSet::new();

        for msg in selected_history {
            if let ContextMessage::ToolResult {
                tool_name,
                tool_call_id,
                success,
                content,
            } = msg
            {
                if tool_name != "tool_search" || !success {
                    continue;
                }
                if !search_call_ids.contains_key(tool_call_id) {
                    continue;
                }
                for name in Self::parse_tool_names(content) {
                    if self.deferred_specs.contains_key(&name) {
                        still_valid.insert(name);
                    }
                }
            }
        }

        self.discovered_tool_names = still_valid;
    }

    /// 从 tool_search 结果文本中解析工具名
    fn parse_tool_names(content: &str) -> Vec<String> {
        content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("- ") {
                    Some(rest.to_string())
                } else {
                    None
                }
            })
            .collect()
    }

    /// 获取当前已发现且仍有效的工具规格
    pub fn get_discovered_specs(&self) -> Vec<ToolSpec> {
        self.discovered_tool_names
            .iter()
            .filter_map(|name| self.deferred_specs.get(name))
            .cloned()
            .collect()
    }

    /// 构建 deferred tools 提示文本
    pub fn build_reminder(&self) -> String {
        let undiscovered: Vec<&ToolSpec> = self
            .deferred_specs
            .values()
            .filter(|spec| !self.discovered_tool_names.contains(&spec.name))
            .collect();

        if undiscovered.is_empty() {
            return String::new();
        }

        let mut lines = vec!["<system-reminder>".to_string()];
        lines.push("以下工具需要先调用 tool_search 发现后才能使用。当用户提到相关需求时请主动搜索：".to_string());
        for spec in &undiscovered {
            let keywords = spec.keywords.join("/");
            lines.push(format!("- {} (关键词: {}): {}", spec.name, keywords, spec.description));
        }
        lines.push("</system-reminder>".to_string());
        lines.join("\n")
    }

    /// 搜索 deferred tools（关键词匹配）
    pub fn search(&self, query: &str, limit: usize) -> Vec<String> {
        let query_lower = query.to_lowercase();
        let query_terms: Vec<&str> = query_lower.split(['_', '-', ' ']).collect();

        let mut scored: Vec<(&str, i32)> = Vec::new();

        for (name, spec) in &self.deferred_specs {
            if self.discovered_tool_names.contains(name) {
                continue; // 已发现，跳过
            }

            let name_lower = name.to_lowercase();
            let desc_lower = spec.description.to_lowercase();
            let mut score = 0i32;

            // 精确工具名匹配
            if query_lower == name_lower {
                score += 1000;
            }
            // 名称前缀
            if name_lower.starts_with(&query_lower) {
                score += 300;
            }
            // 名称子串
            if name_lower.contains(&query_lower) {
                score += 200;
            }
            // 描述子串
            if desc_lower.contains(&query_lower) {
                score += 100;
            }

            // 关键词匹配
            for keyword in &spec.keywords {
                let kw = keyword.to_lowercase();
                if query_lower.contains(&kw) {
                    score += 50;
                }
            }

            // 逐词匹配
            for term in &query_terms {
                if name_lower.contains(term) {
                    score += 25;
                }
                if desc_lower.contains(term) {
                    score += 10;
                }
            }

            // use_when 匹配
            for use_case in &spec.use_when {
                let uc = use_case.to_lowercase();
                if query_lower.contains(&uc) || uc.contains(&query_lower) {
                    score += 40;
                }
            }

            // example_queries 匹配
            for example in &spec.example_queries {
                let ex = example.to_lowercase();
                if query_lower.contains(&ex) || ex.contains(&query_lower) {
                    score += 40;
                }
            }

            if score > 0 {
                scored.push((name.as_str(), score));
            }
        }

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored
            .into_iter()
            .take(limit)
            .map(|(name, _)| name.to_string())
            .collect()
    }

    /// 批量发现工具
    pub fn discover_tools(&mut self, tool_names: &[String]) -> Vec<String> {
        let mut newly = Vec::new();
        for name in tool_names {
            if self.deferred_specs.contains_key(name) && !self.discovered_tool_names.contains(name) {
                self.discovered_tool_names.insert(name.clone());
                newly.push(name.clone());
            }
        }
        newly
    }
}
