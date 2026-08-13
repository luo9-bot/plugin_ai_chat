# Personal Task Progression Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist and advance the bot's own daily and social tasks from real events instead of repeatedly restating thoughts.

**Architecture:** A `personal_tasks` module owns task persistence and validated state transitions. Conversation analysis and self-reflection submit structured task changes; the periodic loop advances due tasks; proactive messaging can only send a due social follow-up through existing guards.

**Tech Stack:** Rust, serde JSON persistence, existing AI tool calling, tracing, cargo test.

---

### Task 1: Task Domain And Store

**Files:**
- Create: `src/personal_tasks/mod.rs`
- Modify: `src/lib.rs`
- Test: `src/personal_tasks/mod.rs`

- [ ] Define `PersonalTask`, `TaskStatus`, and `TaskStore`, including source, next action, associated user/group, timestamps, retry count, and progress log.
- [ ] Implement load/save, candidate merge, status transitions, due-task selection, and bounded history.
- [ ] Test duplicate candidates merge into one task, invalid transitions are rejected, and a completed task cannot return to waiting.

### Task 2: AI Task Changes

**Files:**
- Modify: `src/ai/tools.rs`
- Create: `defaults/task_progress.prompt`
- Modify: `src/personal_tasks/mod.rs`

- [ ] Add a structured `task_progress` tool accepting create/update/complete/snooze/abandon changes.
- [ ] Analyze a completed conversation against current tasks and apply only validated changes.
- [ ] Test unknown IDs are ignored and waiting tasks become active when their associated person replies.

### Task 3: Runtime Integration

**Files:**
- Modify: `src/conversation/handler.rs`
- Modify: `src/self_memory/inner_thought.rs`
- Modify: `src/lib.rs`

- [ ] Queue task analysis after sent replies, with the original user/group context.
- [ ] Include active task progress in inner-thought context without demanding a topic switch.
- [ ] Run due-task review from the periodic loop; mark expired waits as ready to decide, not completed.
- [ ] Test due-task review respects the retry limit and snoozed tasks stay hidden until due.

### Task 4: Task-Driven Proactive Follow-Up

**Files:**
- Modify: `src/proactive/trigger.rs`
- Modify: `src/proactive/generate.rs`
- Modify: `src/personal_tasks/mod.rs`

- [ ] Select at most one due task associated with the current user/group before generic proactive paths.
- [ ] Generate a task-specific follow-up, then pass it through existing duplicate, reply-status, and safe-send checks.
- [ ] Record successful sends as task progress and move the task to waiting with a later review time.
- [ ] Test a follow-up is unavailable before due time and unavailable after the retry limit.

### Task 5: Verification

**Files:**
- Test: `src/personal_tasks/mod.rs`

- [ ] Run `cargo test personal_tasks::tests --lib`.
- [ ] Run `cargo check --lib`.
- [ ] Run `rustfmt --check` for each modified Rust file and `git diff --check` for task files.
