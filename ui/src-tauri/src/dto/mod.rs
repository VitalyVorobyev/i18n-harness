//! IPC wire-format types exchanged with the React UI. Each submodule
//! mirrors a feature bucket from the Tauri command surface.

pub mod catalog;
pub mod correction;
#[cfg(feature = "ollama")]
pub mod eval;
pub mod gate;
pub mod glossary;
pub mod metrics;
pub mod project;
pub mod reuse;
#[cfg(feature = "ollama")]
pub mod translate;
pub mod tuning;

pub use catalog::*;
pub use correction::*;
#[cfg(feature = "ollama")]
pub use eval::*;
pub use gate::*;
pub use glossary::*;
pub use metrics::*;
pub use project::*;
pub use reuse::*;
#[cfg(feature = "ollama")]
pub use translate::*;
pub use tuning::*;
