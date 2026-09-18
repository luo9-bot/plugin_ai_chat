//! 内嵌的管理后台页面
//!
//! 直接引用前端构建产物，而不是把 234KB 的 HTML 复制成 Rust 字面量：
//! - 复制会引入"前端改了但 ui.rs 没重建"的同步义务，而构建脚本漏跑一次
//!   就静默失效（实测发生过）；`include_str!` 让 rustc 自己跟踪该文件；
//! - 原先用 `r##"…"##` 包裹整份 HTML，产物里出现 `"##` 就会让编译失败；
//! - 生成出来的巨大字面量也不必再进版本库。
//!
//! 产物缺失或比源码旧时的处理见 `build.rs`：它会重建 dist，失败即让构建失败。

pub const HTML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/frontend/dist/index.html"
));
