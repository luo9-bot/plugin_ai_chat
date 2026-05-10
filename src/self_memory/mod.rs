mod store;
mod reflect;
mod sync;

// ── re-exports ────────────────────────────────────────────────

// store.rs
pub use store::{
    ThoughtCategory, SelfThought, SelfMemoryStore,
    load_count, add, total_count, correct, get_context,
};

// reflect.rs
pub use reflect::{GroupProfile, reflect};

// sync.rs
pub use sync::{
    sync_to_remote, register_to_registry, sync_all_to_remote,
    remote_list_all, remote_search, remote_search_delete,
    remote_delete, remote_restore, remote_list_deleted,
    remote_purge, remote_stats,
};
