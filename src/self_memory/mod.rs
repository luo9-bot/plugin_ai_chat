pub mod inner_thought;
mod reflect;
mod store;
mod sync;

// ── re-exports ────────────────────────────────────────────────

// store.rs
pub use store::{
    SelfMemoryStore, SelfThought, ThoughtCategory, add, correct, get_context, load_count,
    total_count,
};

// reflect.rs
pub use reflect::{GroupProfile, reflect};

// sync.rs
pub use sync::{
    register_to_registry, remote_delete, remote_list_all, remote_list_deleted, remote_purge,
    remote_restore, remote_search, remote_search_delete, remote_stats, sync_all_to_remote,
    sync_to_remote,
};
