use std::collections::HashMap;
use crate::runtime::request_kind::RequestKind;
use crate::ai::{Tool, FunctionDef};

/// 工具可见性
#[derive(Debug, Clone, PartialEq)]
pub enum ToolVisibility {
    /// 始终可见
    Visible,
    /// 需要通过 tool_search 发现后方可见
    Deferred,
}

/// 工具在哪个阶段可见
#[derive(Debug, Clone, PartialEq)]
pub enum ToolStage {
    /// Timing Gate 阶段
    TimingGate,
    /// Planner 的 Action Loop 阶段
    PlannerAction,
    /// 两个阶段都可见
    Both,
}

/// 工具规格定义
#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    pub visibility: ToolVisibility,
    pub stage: ToolStage,
    pub keywords: Vec<String>,
    pub category: String,
    pub use_when: Vec<String>,
    pub example_queries: Vec<String>,
}

impl ToolSpec {
    /// 转换为 AI API 的 Tool 格式
    pub fn to_llm_tool(&self) -> Tool {
        Tool {
            tool_type: "function".into(),
            function: FunctionDef {
                name: self.name.clone(),
                description: self.description.clone(),
                parameters: self.parameters_schema.clone(),
            },
        }
    }

    /// 创建 visible 工具规格的快捷方法
    pub fn visible(
        name: impl Into<String>,
        description: impl Into<String>,
        params: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters_schema: params,
            visibility: ToolVisibility::Visible,
            stage: ToolStage::PlannerAction,
            keywords: Vec::new(),
            category: String::new(),
            use_when: Vec::new(),
            example_queries: Vec::new(),
        }
    }

    /// 创建 deferred 工具规格的快捷方法
    pub fn deferred(
        name: impl Into<String>,
        description: impl Into<String>,
        params: serde_json::Value,
        keywords: Vec<&str>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            parameters_schema: params,
            visibility: ToolVisibility::Deferred,
            stage: ToolStage::PlannerAction,
            keywords: keywords.into_iter().map(String::from).collect(),
            category: String::new(),
            use_when: Vec::new(),
            example_queries: Vec::new(),
        }
    }
}

/// 工具可用性上下文
#[derive(Debug, Clone)]
pub struct ToolAvailabilityContext {
    pub request_kind: RequestKind,
    pub group_id: u64,
    pub user_id: u64,
    pub is_group_chat: bool,
}

impl ToolAvailabilityContext {
    pub fn new(kind: RequestKind, group_id: u64, user_id: u64) -> Self {
        Self {
            request_kind: kind,
            group_id,
            user_id,
            is_group_chat: group_id > 0,
        }
    }
}

/// 工具调用上下文
pub struct ToolExecutionContext {
    pub session_id: String,
    pub user_id: u64,
    pub group_id: u64,
    pub reasoning_content: Option<String>,
}

/// 工具执行结果
pub struct ToolExecutionResult {
    pub success: bool,
    pub content: String,
    pub structured_content: Option<serde_json::Value>,
}

/// 工具 Provider trait
pub trait ToolProvider: Send + Sync {
    fn list_tools(&self, ctx: &ToolAvailabilityContext) -> Vec<ToolSpec>;
    fn get_tool(&self, name: &str) -> Option<ToolSpec>;
    fn invoke(&self, name: &str, args: &serde_json::Value, ctx: &ToolExecutionContext) -> ToolExecutionResult;
}

/// 统一工具注册表
pub struct ToolRegistry {
    providers: Vec<Box<dyn ToolProvider>>,
    specs_by_name: HashMap<String, ToolSpec>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
            specs_by_name: HashMap::new(),
        }
    }

    /// 注册 Provider，自动索引其工具规格
    pub fn register_provider(&mut self, provider: Box<dyn ToolProvider>) {
        let ctx = ToolAvailabilityContext::new(RequestKind::Planner, 0, 0);
        let specs = provider.list_tools(&ctx);
        for spec in specs {
            self.specs_by_name.insert(spec.name.clone(), spec);
        }
        self.providers.push(provider);
    }

    /// 按可见性列出当前请求可用的工具
    pub fn list_visible_tools(&self, ctx: &ToolAvailabilityContext) -> Vec<ToolSpec> {
        self.specs_by_name
            .values()
            .filter(|spec| {
                match ctx.request_kind {
                    RequestKind::TimingGate => spec.stage == ToolStage::TimingGate || spec.stage == ToolStage::Both,
                    _ => spec.stage == ToolStage::PlannerAction || spec.stage == ToolStage::Both,
                }
            })
            .filter(|spec| {
                // Timing Gate 默认只看到 visible 工具（deferred 不参与节奏判断）
                if ctx.request_kind == RequestKind::TimingGate {
                    return spec.visibility == ToolVisibility::Visible;
                }
                true
            })
            .cloned()
            .collect()
    }

    /// 列出所有 deferred 工具规格
    pub fn list_deferred_specs(&self) -> Vec<ToolSpec> {
        self.specs_by_name
            .values()
            .filter(|spec| spec.visibility == ToolVisibility::Deferred)
            .cloned()
            .collect()
    }

    /// 获取单个工具规格
    pub fn get_tool_spec(&self, name: &str) -> Option<ToolSpec> {
        self.specs_by_name.get(name).cloned()
    }

    /// 执行工具
    pub fn invoke(&self, name: &str, args: &serde_json::Value, ctx: &ToolExecutionContext) -> ToolExecutionResult {
        for provider in &self.providers {
            if provider.get_tool(name).is_some() {
                return provider.invoke(name, args, ctx);
            }
        }
        ToolExecutionResult {
            success: false,
            content: format!("工具 {} 未注册", name),
            structured_content: None,
        }
    }

    /// 将 visible 工具转为 LLM Tool 定义列表
    pub fn visible_tools_to_llm(&self, ctx: &ToolAvailabilityContext) -> Vec<Tool> {
        self.list_visible_tools(ctx)
            .iter()
            .map(|spec| spec.to_llm_tool())
            .collect()
    }

    /// 将一组额外工具规格转为 LLM Tool 定义列表
    pub fn specs_to_llm(specs: &[ToolSpec]) -> Vec<Tool> {
        specs.iter().map(|spec| spec.to_llm_tool()).collect()
    }
}
