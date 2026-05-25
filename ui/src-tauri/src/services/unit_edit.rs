//! State-machine helpers shared by the file-mode and project-mode
//! unit-edit commands.
//!
//! `apply_target_edit` enforces:
//!   - the unit is UI-editable (not Vanished/Obsolete)
//!   - the edit kind (Singular/Plural) matches the target shape
//!   - plural form indices stay in range
//!   - the Untranslated ↔ Proposed ↔ Finished transitions:
//!     Untranslated → Proposed once any text is set;
//!     Proposed/Finished → Untranslated once the target is fully cleared (flags reset);
//!     Finished → Proposed when text changes (human reconsidering).

use i18n_harness_core::{Target, Unit, UnitState};

use crate::dto::catalog::TargetEdit;

/// Apply one `TargetEdit` to `unit`, enforcing the state-machine transitions.
///
/// Returns `Err` with a human-readable message if the edit is rejected
/// (non-editable state, mismatched kind, or out-of-range plural index).
/// The caller is responsible for marking the containing catalog dirty.
pub(crate) fn apply_target_edit(unit: &mut Unit, edit: TargetEdit) -> Result<(), String> {
    if !unit.state.is_ui_editable() {
        return Err(format!(
            "unit {id} is {state:?} — vanished/obsolete units are not editable",
            id = unit.id,
            state = unit.state,
        ));
    }
    match (&mut unit.target, edit) {
        (Target::Singular { text }, TargetEdit::Singular { text: new }) => *text = new,
        (Target::Plural { forms }, TargetEdit::Plural { form_index, text }) => {
            let i = form_index as usize;
            if i >= forms.len() {
                return Err(format!(
                    "plural form index {i} out of range (have {})",
                    forms.len()
                ));
            }
            forms[i] = text;
        }
        (Target::Singular { .. }, TargetEdit::Plural { .. }) => {
            return Err("cannot apply plural edit to singular unit".into());
        }
        (Target::Plural { .. }, TargetEdit::Singular { .. }) => {
            return Err("cannot apply singular edit to plural unit".into());
        }
    }
    // State auto-transitions on edit:
    //   Untranslated          → Proposed      once any text is set
    //   Proposed | Finished   → Untranslated  once the target is fully cleared
    //   Finished              → Proposed      when text changes (human reconsidering)
    match unit.state {
        UnitState::Untranslated if !unit.target.is_empty() => {
            unit.state = UnitState::Proposed;
        }
        UnitState::Proposed | UnitState::Finished if unit.target.is_empty() => {
            unit.state = UnitState::Untranslated;
            unit.flags = Default::default();
        }
        UnitState::Finished if !unit.target.is_empty() => {
            // editing a Finished unit reverts to Proposed — the
            // human is actively reconsidering the finalized translation
            unit.state = UnitState::Proposed;
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use i18n_harness_core::{Flag, FlagSet};

    fn singular_unit(id: &str, text: Option<&str>) -> Unit {
        let mut u = Unit::untranslated_singular(id, "source text");
        u.target = Target::Singular {
            text: text.map(str::to_owned),
        };
        u
    }

    fn plural_unit(id: &str, forms: Vec<Option<&str>>) -> Unit {
        let mut u = Unit::untranslated_singular(id, "source text");
        u.target = Target::Plural {
            forms: forms.into_iter().map(|s| s.map(str::to_owned)).collect(),
        };
        u.plural_arity = Some(2);
        u
    }

    #[test]
    fn singular_text_set_on_untranslated_transitions_to_proposed() {
        let mut unit = singular_unit("u1", None);
        assert_eq!(unit.state, UnitState::Untranslated);
        apply_target_edit(
            &mut unit,
            TargetEdit::Singular {
                text: Some("Hallo".to_owned()),
            },
        )
        .unwrap();
        assert_eq!(unit.state, UnitState::Proposed);
        assert_eq!(
            unit.target,
            Target::Singular {
                text: Some("Hallo".to_owned())
            }
        );
    }

    #[test]
    fn singular_text_cleared_on_proposed_reverts_to_untranslated_and_clears_flags() {
        let mut unit = singular_unit("u2", Some("Hallo"));
        unit.state = UnitState::Proposed;
        let mut fs = FlagSet::new();
        fs.insert(Flag::PlaceholderMismatch);
        unit.flags = fs;

        apply_target_edit(&mut unit, TargetEdit::Singular { text: None }).unwrap();

        assert_eq!(unit.state, UnitState::Untranslated);
        assert!(unit.flags.is_empty(), "flags should be cleared");
    }

    #[test]
    fn finished_with_non_empty_edit_reverts_to_proposed() {
        let mut unit = singular_unit("u3", Some("Alt"));
        unit.state = UnitState::Finished;

        apply_target_edit(
            &mut unit,
            TargetEdit::Singular {
                text: Some("Neu".to_owned()),
            },
        )
        .unwrap();

        assert_eq!(unit.state, UnitState::Proposed);
        assert_eq!(
            unit.target,
            Target::Singular {
                text: Some("Neu".to_owned())
            }
        );
    }

    #[test]
    fn finished_cleared_reverts_to_untranslated() {
        let mut unit = singular_unit("u4", Some("Alt"));
        unit.state = UnitState::Finished;

        apply_target_edit(&mut unit, TargetEdit::Singular { text: None }).unwrap();

        assert_eq!(unit.state, UnitState::Untranslated);
        assert!(unit.flags.is_empty());
    }

    #[test]
    fn plural_form_set_at_valid_index_keeps_other_forms() {
        let mut unit = plural_unit("u5", vec![None, None]);
        unit.state = UnitState::Untranslated;

        apply_target_edit(
            &mut unit,
            TargetEdit::Plural {
                form_index: 1,
                text: Some("eins".to_owned()),
            },
        )
        .unwrap();

        // Only form 1 changed; form 0 untouched.
        assert_eq!(
            unit.target,
            Target::Plural {
                forms: vec![None, Some("eins".to_owned())]
            }
        );
        // Target still has a None so is_empty is not satisfied — stays Untranslated
        // because target.is_empty() returns true only when ALL forms are None.
        // With form[0] = None and form[1] = Some("eins"), is_empty() = false,
        // so state transitions to Proposed.
        assert_eq!(unit.state, UnitState::Proposed);
    }

    #[test]
    fn plural_form_index_out_of_range_returns_error() {
        let mut unit = plural_unit("u6", vec![None, None]);

        let result = apply_target_edit(
            &mut unit,
            TargetEdit::Plural {
                form_index: 5,
                text: Some("x".to_owned()),
            },
        );

        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("out of range"),
            "expected 'out of range' in: {msg}"
        );
    }

    #[test]
    fn singular_edit_on_plural_target_returns_error() {
        let mut unit = plural_unit("u7", vec![None, None]);

        let result = apply_target_edit(
            &mut unit,
            TargetEdit::Singular {
                text: Some("x".to_owned()),
            },
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "cannot apply singular edit to plural unit"
        );
    }

    #[test]
    fn plural_edit_on_singular_target_returns_error() {
        let mut unit = singular_unit("u8", None);

        let result = apply_target_edit(
            &mut unit,
            TargetEdit::Plural {
                form_index: 0,
                text: Some("x".to_owned()),
            },
        );

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            "cannot apply plural edit to singular unit"
        );
    }

    #[test]
    fn non_ui_editable_state_returns_error() {
        let mut unit = singular_unit("vanished_unit", Some("text"));
        unit.state = UnitState::Vanished;

        let result = apply_target_edit(
            &mut unit,
            TargetEdit::Singular {
                text: Some("new".to_owned()),
            },
        );

        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert_eq!(
            msg,
            "unit vanished_unit is Vanished — vanished/obsolete units are not editable"
        );

        // Also test Obsolete.
        let mut unit2 = singular_unit("obs", Some("text"));
        unit2.state = UnitState::Obsolete;
        let result2 = apply_target_edit(
            &mut unit2,
            TargetEdit::Singular {
                text: Some("new".to_owned()),
            },
        );
        assert!(result2.is_err());
        let msg2 = result2.unwrap_err();
        assert_eq!(
            msg2,
            "unit obs is Obsolete — vanished/obsolete units are not editable"
        );
    }
}
