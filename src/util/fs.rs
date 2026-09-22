//! 文件写入
//!
//! 状态文件一律走 [`atomic_write`]：`AGENTS.md` §可读性与维护性 要求
//! "文件替换必须保留突然中断后的恢复能力"。直接覆盖的失败模式很隐蔽——
//! 中断留下半个 JSON，下次启动时 `serde_json` 解析失败，
//! 整个文件的内容（可能是她几个月的记忆）被当成空文件处理。

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// 临时文件名的序号，保证同进程内并发写同一个目标也不会互相覆盖
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// 临时文件路径：与目标同目录（rename 只在同一文件系统内原子）
fn temp_path(path: &Path) -> PathBuf {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "state".to_string());
    let seq = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(".{name}.tmp-{}-{seq}", std::process::id()))
}

/// 原子替换文件内容：临时文件 → fsync → rename
///
/// `sync_all` 必须在 rename **之前**：否则崩溃后可能 rename 出一个
/// 内容还在页缓存里的空文件，反而比"没有这次写入"更糟。
///
/// 两个参数都接受 `AsRef`，与 `std::fs::write` 保持一致，
/// 这样替换既有调用点时不需要改参数写法。
pub(crate) fn atomic_write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let path = path.as_ref();
    let contents = contents.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = temp_path(path);
    let write_result = File::create(&tmp).and_then(|mut file| {
        file.write_all(contents)?;
        file.sync_all()
    });
    if let Err(error) = write_result {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }

    if let Err(error) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ai_chat_atomic_{tag}_{}_{}",
            std::process::id(),
            TEMP_SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).expect("建临时目录");
        dir
    }

    #[test]
    fn replaces_existing_content() {
        let dir = temp_dir("replace");
        let path = dir.join("state.json");

        atomic_write(&path, b"first").expect("首次写入");
        assert_eq!(fs::read(&path).expect("读取"), b"first");

        atomic_write(&path, b"second").expect("覆盖写入");
        assert_eq!(fs::read(&path).expect("读取"), b"second");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn creates_missing_parent_directories() {
        let dir = temp_dir("mkdir");
        let path = dir.join("nested").join("deeper").join("state.json");

        atomic_write(&path, b"ok").expect("应自动建目录");
        assert_eq!(fs::read(&path).expect("读取"), b"ok");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn leaves_no_temp_files_behind() {
        let dir = temp_dir("clean");
        let path = dir.join("state.json");
        atomic_write(&path, b"x").expect("写入");

        let leftovers: Vec<String> = fs::read_dir(&dir)
            .expect("列目录")
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".tmp-"))
            .collect();
        assert!(leftovers.is_empty(), "残留临时文件：{leftovers:?}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn concurrent_writers_never_produce_a_partial_file() {
        let dir = temp_dir("concurrent");
        let path = dir.join("state.json");
        let payloads: Vec<String> = (0..8)
            .map(|i| format!("payload-{i}-{}", "x".repeat(4096)))
            .collect();

        let handles: Vec<_> = payloads
            .clone()
            .into_iter()
            .map(|payload| {
                let path = path.clone();
                std::thread::spawn(move || atomic_write(&path, payload.as_bytes()))
            })
            .collect();
        for handle in handles {
            handle.join().expect("写线程不应 panic").expect("写入成功");
        }

        // 结果必须是某一次写入的**完整**内容，而不是交错的字节
        let final_bytes = fs::read(&path).expect("读取");
        let final_text = String::from_utf8(final_bytes).expect("必须是合法 UTF-8");
        assert!(
            payloads.contains(&final_text),
            "最终内容不是任何一次完整写入：{final_text:.40}"
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
