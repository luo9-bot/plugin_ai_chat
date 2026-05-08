use std::path::Path;

fn main() {
    let dist = Path::new("frontend/dist/index.html");
    let out = Path::new("src/admin_ui.rs");

    // 仅当 dist/index.html 存在且比 admin_ui.rs 更新时才重新生成
    let should_rebuild = if dist.exists() {
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

    if should_rebuild {
        let html = std::fs::read_to_string(dist).expect("failed to read frontend/dist/index.html");
        let rs = format!("pub const HTML: &str = r##\"{}\"##;\n", html);
        let size = rs.len();
        std::fs::write(out, &rs).expect("failed to write src/admin_ui.rs");
        println!("cargo:warning=admin_ui.rs regenerated from frontend/dist/index.html ({:.1} KB)", size as f64 / 1024.0);
    }

    // 如果 admin_ui.rs 不存在且 dist 也不存在，创建占位
    if !out.exists() {
        let fallback = "pub const HTML: &str = r##\"<!DOCTYPE html><html><body><h1>Frontend not built. Run: cd frontend && npm run build</h1></body></html>\"##;\n";
        std::fs::write(out, fallback).expect("failed to write placeholder admin_ui.rs");
    }

    // 告诉 cargo 监视这些文件
    println!("cargo:rerun-if-changed=frontend/dist/index.html");
    println!("cargo:rerun-if-changed=src/admin_ui.rs");
}
