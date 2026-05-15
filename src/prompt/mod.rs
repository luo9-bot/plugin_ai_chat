//! Prompt 管理系统：外部化所有 AI prompt，支持占位符替换和热重载

mod manager;
pub mod renderer;

pub use manager::PromptManager;
pub use renderer::PromptRenderer;
