//! Qt Linguist `.ts` adapter — the reference adapter.
//!
//! Round-trip fidelity dominates; uses `quick-xml` with hand-managed
//! serialization. See `docs/initial_design.md` §7 and §13.1.
//! Phase 0 stub — parser, writer, placeholder normalizer (Qt `%1`/`%n` ↔
//! ICU `{1}`/`{count}`), and round-trip tests land in M0.
