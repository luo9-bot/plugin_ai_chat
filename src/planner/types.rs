//! Planner 类型定义

#[derive(Debug)]
pub enum PlannerAction {
    Reply { user_id: u64, group_id: u64, message: String, reference_info: Option<String> },
    Silent,
}

pub struct PlannerContext {
    pub group_id: u64,
    pub user_id: u64,
    pub user_message: String,
    pub identity: String,
    pub extra_context: String,
    pub history: Vec<(String, String)>,
}
