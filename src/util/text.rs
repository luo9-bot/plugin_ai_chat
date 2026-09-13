//! 文本比较和解析工具

/// 解析管理员命令的 QQ 号参数
pub fn parse_uid_arg(msg: &str, prefix: &str) -> Option<Result<u64, String>> {
    let rest = msg.strip_prefix(prefix)?;
    match rest.trim().parse::<u64>() {
        Ok(uid) => Some(Ok(uid)),
        Err(_) => Some(Err(format!("格式: {}QQ号", prefix))),
    }
}
