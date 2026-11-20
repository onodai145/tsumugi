//! 内蔵 VOICEVOX CORE(Issue #625 段階2)。設計は `docs/superpowers/specs/2026-11-20-issue-625-voicevox-core-stage2-design.md`。

pub mod catalog;
pub mod store;
pub mod archive;
#[cfg(test)]
pub(crate) mod test_util;
