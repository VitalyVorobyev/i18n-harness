//! Strings that occur in several catalogs of the same language.
//!
//! Two units are the same string when their catalogs have the same target
//! locale and the units have the same id and the same original. The original
//! is part of the key because some formats reuse a key for different text
//! (an ICU-JSON key, a gettext msgid edited upstream).

use std::collections::HashMap;

use i18n_harness_core::{Target, Unit};

use crate::session::{OpenCatalog, is_active};

/// Position of a unit: catalog slot, unit index.
pub(crate) type At = (usize, usize);

/// Where each string occurs, over every open catalog.
pub(crate) struct SharedIndex {
    groups: Vec<Vec<At>>,
    group_of: HashMap<At, usize>,
}

/// How one string relates to the same string in other files.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct SharedInfo {
    /// The string occurs in another file.
    pub(crate) shared: bool,
    /// The files that translate it do not all agree.
    pub(crate) inconsistent: bool,
}

impl SharedIndex {
    pub(crate) fn build(catalogs: &[OpenCatalog]) -> Self {
        let mut by_key: HashMap<(&str, &str, &str), usize> = HashMap::new();
        let mut groups: Vec<Vec<At>> = Vec::new();
        let mut group_of = HashMap::new();
        for (slot, catalog) in catalogs.iter().enumerate() {
            for (index, unit) in catalog.doc.units().iter().enumerate() {
                if !is_active(unit) {
                    continue;
                }
                let key = (catalog.locale.id, unit.id.0.as_str(), unit.source.as_str());
                let group = *by_key.entry(key).or_insert_with(|| {
                    groups.push(Vec::new());
                    groups.len() - 1
                });
                groups[group].push((slot, index));
                group_of.insert((slot, index), group);
            }
        }
        Self { groups, group_of }
    }

    pub(crate) fn info(&self, catalogs: &[OpenCatalog], at: At) -> SharedInfo {
        let Some(&group) = self.group_of.get(&at) else {
            return SharedInfo::default();
        };
        let members = &self.groups[group];
        if members.len() < 2 {
            return SharedInfo::default();
        }
        let mut first: Option<&Target> = None;
        let mut inconsistent = false;
        for &(slot, index) in members {
            let catalog = &catalogs[slot];
            let unit: &Unit = &catalog.doc.units()[index];
            if !catalog.is_translation(unit) {
                continue;
            }
            match first {
                None => first = Some(&unit.target),
                Some(target) if *target != unit.target => {
                    inconsistent = true;
                    break;
                }
                Some(_) => {}
            }
        }
        SharedInfo {
            shared: true,
            inconsistent,
        }
    }
}
