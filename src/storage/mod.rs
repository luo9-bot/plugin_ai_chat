//! 统一存储层
//!
//! 所有持久化数据统一使用 SQLite 存储。
//! 提供统一的连接管理和迁移工具。

pub mod sqlite;

/// 初始化存储层
pub fn init(data_dir: &std::path::Path) {
    sqlite::init(data_dir);
}
