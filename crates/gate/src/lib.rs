//! Validation gate: the trust boundary between LLM output and catalog write-back.
//!
//! See `docs/initial_design.md` §8. Phase 0 stub — hard checks
//! (placeholder multiset, plural/select arity, ICU parse) and soft checks
//! (accelerator, length warn, CJK punctuation, agreement risk) land in M1.
