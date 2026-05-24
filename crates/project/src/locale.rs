//! Resolved locale view — three-layer merge of workspace data, manifest
//! overrides, and glossary overrides.
//!
//! See `docs/m4.1-project-crate-design.md` §1.6 and §4.

use i18n_harness_glossary::Glossary;
use i18n_harness_locales::{Locale, PluralCategory, Register, Script};

use crate::manifest::{LocaleConfig, RegisterOverride};

/// The merged view of one locale: workspace-immutable fields from
/// `crates/locales`, project-overridable fields from the manifest, and
/// glossary-overridable fields.
///
/// # Layer priority (highest wins)
///
/// 1. **Glossary** `[locale.<id>]` — `register`, `variant`.
/// 2. **Manifest** `[locales.<id>]` — `register`, `variant`, `length_warn_ratio`.
/// 3. **Workspace** (`crates/locales`) — all fields, including immutable ones.
///
/// # Immutable fields
///
/// `cldr_plural`, `plural_arity`, and `script` always come from the workspace
/// record. A locale's CLDR arity and script are facts about the language, not
/// project policy.
///
/// # `workspace_locale()` caveat
///
/// `workspace_locale()` returns the raw `&'static Locale`. Its `register`,
/// `variant`, and `length_warn_ratio` fields reflect the **workspace** values,
/// not the resolved overrides. Callers that need the effective values must read
/// them through `self.register()`, `self.variant()`, and
/// `self.length_warn_ratio()` rather than through the returned reference.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedLocale {
    workspace: &'static Locale,
    /// Effective register after merging manifest and glossary.
    effective_register: Register,
    /// Effective variant after merging manifest and glossary. Points to either
    /// a `&'static str` (workspace default) or a leaked `String` from the
    /// manifest / glossary. We store it as `&'static str` by leaking on
    /// construction so the struct stays `Copy`.
    effective_variant: &'static str,
    /// Effective length_warn_ratio: manifest overrides workspace; glossary
    /// cannot override this field (it has no `length_warn_ratio` field).
    effective_length_warn_ratio: f32,
}

impl ResolvedLocale {
    /// Merge the three layers into a resolved locale view.
    ///
    /// Returns `None` if `id` is not in the workspace locale table (callers
    /// that need a resolved view without a workspace record cannot produce one
    /// — there is no honest default for `plural_arity` or `script`).
    /// Merge the three layers into a resolved locale view.
    ///
    /// Returns `None` if `id` is not in the workspace locale table — there is
    /// no honest default for `plural_arity` or `script`.
    ///
    /// Callable directly from tests that need a `ResolvedLocale` without a
    /// full `Project`.
    pub fn resolve(
        id: &str,
        manifest: Option<&LocaleConfig>,
        glossary: Option<&Glossary>,
    ) -> Option<Self> {
        let workspace = Locale::by_id(id)?;

        // Layer 1: workspace defaults.
        let mut effective_register = workspace.register;
        let mut effective_variant: &'static str = workspace.variant;
        let mut effective_length_warn_ratio = workspace.length_warn_ratio;

        // Layer 2: manifest overrides.
        if let Some(cfg) = manifest {
            if let Some(r) = cfg.register {
                effective_register = register_override_to_register(r);
            }
            if let Some(v) = &cfg.variant {
                // Leak the string so we can store a `&'static str`.
                // This allocation is bounded: at most one per resolved locale
                // per open project, and locales are few.
                effective_variant = Box::leak(v.clone().into_boxed_str());
            }
            if let Some(ratio) = cfg.length_warn_ratio {
                effective_length_warn_ratio = ratio;
            }
        }

        // Layer 3: glossary overrides (highest priority for register/variant).
        if let Some(g) = glossary {
            if let Some(r) = g.register_for(id) {
                effective_register = glossary_register_to_register(r);
            }
            if let Some(v) = g.variant_for(id) {
                effective_variant = Box::leak(v.to_string().into_boxed_str());
            }
            // Glossary has no length_warn_ratio field — manifest is final.
        }

        Some(Self {
            workspace,
            effective_register,
            effective_variant,
            effective_length_warn_ratio,
        })
    }

    /// Locale id (e.g. `"de_DE"`).
    pub fn id(&self) -> &str {
        self.workspace.id
    }

    /// CLDR plural categories for cardinal numbers. **Workspace-immutable.**
    pub fn cldr_plural(&self) -> &'static [PluralCategory] {
        self.workspace.cldr_plural
    }

    /// Plural arity (number of CLDR plural categories). **Workspace-immutable.**
    pub fn plural_arity(&self) -> u32 {
        self.workspace.plural_arity()
    }

    /// Script family. **Workspace-immutable.**
    pub fn script(&self) -> Script {
        self.workspace.script
    }

    /// Effective register after applying manifest and glossary overrides.
    pub fn register(&self) -> Register {
        self.effective_register
    }

    /// Effective variant tag after applying manifest and glossary overrides.
    pub fn variant(&self) -> &str {
        self.effective_variant
    }

    /// Effective length-warn ratio after applying the manifest override.
    ///
    /// The glossary cannot override this value — it has no `length_warn_ratio`
    /// field by design.
    pub fn length_warn_ratio(&self) -> f32 {
        self.effective_length_warn_ratio
    }

    /// Borrow the underlying workspace [`Locale`] for callers that accept
    /// `&Locale` (the gate today takes `&Locale`).
    ///
    /// **Caveat:** the returned `&Locale` carries the *workspace* register,
    /// variant, and length_warn_ratio — not the resolved overrides. Callers
    /// that need effective values must use `self.register()` etc. rather than
    /// reading through the returned reference. We cannot synthesize a fresh
    /// `&'static Locale` because `Locale` fields are `&'static str` and that
    /// would require leaking memory for every resolved value.
    pub fn workspace_locale(&self) -> &'static Locale {
        self.workspace
    }
}

/// Placeholder for the full `ResolvedLocaleView` (slice c+). Defined here so
/// the re-export in `lib.rs` compiles even before the full view is implemented.
///
/// The full view adds iteration helpers and batch accessors; this stub allows
/// downstream code to compile against the type name.
pub type ResolvedLocaleView = ResolvedLocale;

// ── Conversion helpers ────────────────────────────────────────────────────────

fn register_override_to_register(r: RegisterOverride) -> Register {
    match r {
        RegisterOverride::Formal => Register::Formal,
        RegisterOverride::Informal => Register::Informal,
        RegisterOverride::Neutral => Register::Neutral,
    }
}

fn glossary_register_to_register(r: i18n_harness_glossary::Register) -> Register {
    r.to_locales_register()
}
