//! 锁访问：中毒时恢复而不是 panic
//!
//! `Mutex`/`RwLock` 中毒只意味着**持锁线程 panic 过**，受保护的数据可能仍然
//! 自洽（例如临界区已经写完、只是返回值时崩了）。而 `.lock_recover()` 会把
//! 那一次 panic 变成**之后每一次**访问都 panic。
//!
//! 在本仓的调用栈上这一点格外致命：插件入口是 `extern "C"`，一次 panic
//! 不会刷新日志，而 `PROCESSING_USERS` 这类跨线程守卫一旦被毒化，
//! 之后每条消息都会在取锁时 panic——插件表现为"活着但什么都不做"。
//!
//! 因此这里统一提供 `*_recover()`：取回内部值继续用，把"中毒"降级为
//! 一次数据可能不完整的风险，而不是全局停摆。

use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

/// `Mutex::lock` 的中毒容忍版本
pub(crate) trait MutexExt<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for Mutex<T> {
    fn lock_recover(&self) -> MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// `RwLock::read` 的中毒容忍版本
pub(crate) trait RwLockReadExt<T> {
    fn read_recover(&self) -> RwLockReadGuard<'_, T>;
}

impl<T> RwLockReadExt<T> for RwLock<T> {
    fn read_recover(&self) -> RwLockReadGuard<'_, T> {
        self.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// `RwLock::write` 的中毒容忍版本
pub(crate) trait RwLockWriteExt<T> {
    fn write_recover(&self) -> RwLockWriteGuard<'_, T>;
}

impl<T> RwLockWriteExt<T> for RwLock<T> {
    fn write_recover(&self) -> RwLockWriteGuard<'_, T> {
        self.write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 毒化之后仍然可读可写——这正是本模块存在的理由
    #[test]
    fn poisoned_mutex_still_yields_its_value() {
        let mutex = Mutex::new(7u32);

        // 模拟一个持锁线程 panic
        let poison_result = std::panic::catch_unwind(|| {
            let _guard = mutex.lock().expect("首次加锁");
            panic!("模拟持锁线程 panic");
        });
        assert!(poison_result.is_err(), "这里应该捕获到 panic");
        assert!(mutex.lock().is_err(), "锁应已被毒化");

        // 容忍版本仍能取到值，而 `.lock_recover()` 会在这里 panic
        let guard = mutex.lock_recover();
        assert_eq!(*guard, 7);
    }

    #[test]
    fn poisoned_rwlock_still_yields_its_value() {
        let lock = RwLock::new(String::from("state"));

        let poison_result = std::panic::catch_unwind(|| {
            let _guard = lock.write().expect("首次加锁");
            panic!("模拟持锁线程 panic");
        });
        assert!(poison_result.is_err());

        assert_eq!(&*lock.read_recover(), "state");
        lock.write_recover().push_str(" updated");
        assert_eq!(&*lock.read_recover(), "state updated");
    }
}
