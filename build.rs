use std::path::Path;
use std::time::SystemTime;

fn find_files(dir: &Path, files: &mut Vec<std::path::PathBuf>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                find_files(&path, files);
            } else {
                files.push(path);
            }
        }
    }
}

fn sources_changed_since(stamp: SystemTime) -> bool {
    let src_dir = Path::new("frontend/src");
    if !src_dir.exists() {
        return false;
    }
    let mut files = Vec::new();
    find_files(src_dir, &mut files);
    // 也监视配置文件
    for extra in ["frontend/package.json", "frontend/vite.config.js"] {
        if Path::new(extra).exists() {
            files.push(std::path::PathBuf::from(extra));
        }
    }
    files.iter().any(|f| {
        std::fs::metadata(f)
            .and_then(|m| m.modified())
            .map(|t| t > stamp)
            .unwrap_or(false)
    })
}

fn main() {
    let dist = Path::new("frontend/dist/index.html");
    let out = Path::new("src/admin_ui.rs");
    let stamp = Path::new("frontend/.last-build");

    // ── 1. 检测前端源文件是否变化，自动编译前端 ──
    let stamp_time = std::fs::metadata(stamp)
        .and_then(|m| m.modified())
        .unwrap_or(SystemTime::UNIX_EPOCH);

    if sources_changed_since(stamp_time) {
        println!("cargo:warning=Frontend sources changed, running npm build...");
        // Windows 上 npm 是 .cmd 文件，需要通过 shell 执行
        let (cmd, args) = if cfg!(target_os = "windows") {
            ("cmd", vec!["/c", "npm", "run", "build"])
        } else {
            ("npm", vec!["run", "build"])
        };
        let status = std::process::Command::new(cmd)
            .args(&args)
            .current_dir("frontend")
            .status();
        match status {
            Ok(s) if s.success() => {
                // 删除后重建以确保修改时间更新（Windows 上写入相同内容不会更新 mtime）
                let _ = std::fs::remove_file(stamp);
                std::fs::write(stamp, "").ok();
                println!("cargo:warning=Frontend build succeeded");
            }
            Ok(s) => {
                println!("cargo:warning=Frontend build failed (exit code: {:?}), using existing dist", s.code());
            }
            Err(e) => {
                println!("cargo:warning=Failed to run npm build: {e}, using existing dist");
            }
        }
    }

    // ── 2. 将 dist/index.html 写入 admin_ui.rs ──
    let should_copy = if dist.exists() {
        if out.exists() {
            let dist_meta = std::fs::metadata(dist).unwrap();
            let out_meta = std::fs::metadata(out).unwrap();
            dist_meta.modified().unwrap() > out_meta.modified().unwrap()
        } else {
            true
        }
    } else {
        false
    };

    if should_copy {
        let html = std::fs::read_to_string(dist).expect("failed to read frontend/dist/index.html");
        let rs = format!("pub const HTML: &str = r##\"{}\"##;\n", html);
        let size = rs.len();
        std::fs::write(out, &rs).expect("failed to write src/admin_ui.rs");
        println!("cargo:warning=admin_ui.rs regenerated from dist ({:.1} KB)", size as f64 / 1024.0);
    }

    // 如果 admin_ui.rs 不存在且 dist 也不存在，创建占位
    if !out.exists() {
        let fallback = "pub const HTML: &str = r##\"<!DOCTYPE html><html><body><h1>Frontend not built. Run: cd frontend && npm run build</h1></body></html>\"##;\n";
        std::fs::write(out, fallback).expect("failed to write placeholder admin_ui.rs");
    }

    // ── 3. 告诉 cargo 监视哪些文件 ──
    // 前端源码目录（文件增删会触发，但修改不会——用步骤1的时间戳检测覆盖）
    println!("cargo:rerun-if-changed=frontend/src");
    // dist 产物
    println!("cargo:rerun-if-changed=frontend/dist/index.html");
    // build.rs 自身
    println!("cargo:rerun-if-changed=build.rs");
}
