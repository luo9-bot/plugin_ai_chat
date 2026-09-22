//! 构建脚本：保证 `frontend/dist/index.html` 存在且是新的。
//!
//! **不再生成内嵌 UI 模块。** `src/admin/ui.rs` 现在用
//! `include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/frontend/dist/index.html"))`
//! 直接引用产物，于是"前端变了但内嵌 UI 没更新"由 rustc 自己的依赖跟踪
//! 保证——构建脚本只有在 cargo 判定需要时才运行，靠它维护产物会漏。
//!
//! 这里只负责一件事：产物缺失或比源码旧时重建它，且**失败必须让构建失败**。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 前端产物：单文件 HTML（vite-singlefile 已把 JS/CSS 内联）
const DIST_HTML: &str = "frontend/dist/index.html";
/// 前端源码目录；不存在时说明这是纯 Rust 打包，跳过前端环节
const FRONTEND_SRC: &str = "frontend/src";
/// 与源码一起决定"是否需要重建"的前端配置
const FRONTEND_CONFIGS: [&str; 2] = ["frontend/package.json", "frontend/vite.config.js"];

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// 前端源码里最新的修改时间；没有可监视文件时返回 `None`
fn newest_frontend_mtime() -> Option<SystemTime> {
    let mut files = Vec::new();
    collect_files(Path::new(FRONTEND_SRC), &mut files);
    files.extend(
        FRONTEND_CONFIGS
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.exists()),
    );
    files.iter().filter_map(|f| modified(f)).max()
}

/// 前端源码是否比产物新（产物缺失也算新）
fn frontend_needs_rebuild() -> bool {
    let Some(source_mtime) = newest_frontend_mtime() else {
        return false;
    };
    match modified(Path::new(DIST_HTML)) {
        Some(dist_mtime) => source_mtime > dist_mtime,
        None => true,
    }
}

/// 构建前端。失败即返回 `Err`：`cargo:warning` 不会让构建失败，
/// 而"沿用旧 dist"会让前端改动静默失效。
fn build_frontend() -> Result<(), String> {
    // Windows 上 npm 是 .cmd，需要经 cmd 执行
    let mut command = if cfg!(target_os = "windows") {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/c", "npm", "run", "build"]);
        cmd
    } else {
        let mut cmd = std::process::Command::new("npm");
        cmd.args(["run", "build"]);
        cmd
    };

    let status = command
        .current_dir("frontend")
        .status()
        .map_err(|e| format!("无法执行 `npm run build`：{e}"))?;

    if !status.success() {
        return Err(format!("`npm run build` 失败（{status}）"));
    }
    Ok(())
}

fn main() {
    // 只监视影响"产物是否需要重建"的输入；产物本身由 rustc 的
    // `include_str!` 依赖跟踪，不需要在这里重复声明
    println!("cargo:rerun-if-changed={FRONTEND_SRC}");
    for config in FRONTEND_CONFIGS {
        println!("cargo:rerun-if-changed={config}");
    }
    println!("cargo:rerun-if-changed=build.rs");

    let has_frontend_sources = Path::new(FRONTEND_SRC).exists();
    if !has_frontend_sources {
        // 纯 Rust 打包：没有前端源码可构建，产物由调用方提供
        return;
    }

    if frontend_needs_rebuild() {
        println!("cargo:warning=前端源码有变更，正在重建 dist ...");
        if let Err(error) = build_frontend() {
            panic!(
                "前端构建失败：{error}\n\
                 内嵌 UI 会停留在旧版本，因此这里直接失败而不是沿用旧 dist。\n\
                 修复方式：cd frontend && npm ci && npm run build"
            );
        }
    }

    if !Path::new(DIST_HTML).exists() {
        panic!(
            "{DIST_HTML} 不存在，`src/admin/ui.rs` 通过 include_str! 引用它。\n\
             请先运行：cd frontend && npm ci && npm run build"
        );
    }
}
