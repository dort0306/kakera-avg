//! Generic repeat paging. Pure range planning; no IO, no interpreter, no GPU.
use crate::{
    interface::{Definition, Kind, Node},
    model::{VariableValue, Variables},
};
use std::collections::BTreeMap;

pub const MAX_PAGE_SIZE: usize = 200;
pub const MAX_SLOTS: usize = crate::save::SLOT_COUNT as usize;
pub const MAX_AUTOSAVES: usize = crate::save::AUTO_COUNT as usize;
pub const MAX_HISTORY: usize = crate::model::MAX_HISTORY;
pub const MAX_GALLERY: usize = 256;

#[derive(Clone, Debug)]
pub struct Pagination {
    pub page_size: usize,
    pub variable: String,
}

/// Complete source sizes used to plan one page. The host fills actual values at
/// runtime; loading uses each source's legal maximum for declaration checks.
#[derive(Clone, Copy, Debug, Default)]
pub struct Totals {
    pub choices: usize,
    pub history: usize,
    pub gallery: usize,
    pub variables: usize,
    pub text_locales: usize,
    pub voice_locales: usize,
    pub autosaves: usize,
    pub slots: usize,
}
impl Totals {
    pub fn source(&self, id: &str) -> usize {
        match id {
            "choices" => self.choices,
            "history" => self.history,
            "gallery" => self.gallery,
            "variables" => self.variables,
            "textLocales" => self.text_locales,
            "voiceLocales" => self.voice_locales,
            "autosaves" => self.autosaves,
            "slots" => self.slots,
            _ => 0,
        }
    }
    /// Legal maximum for declaration-time validation (`--check` cannot see saves).
    pub fn legal(source: &str) -> usize {
        match source {
            "slots" => MAX_SLOTS,
            "autosaves" => MAX_AUTOSAVES,
            "history" => MAX_HISTORY,
            "gallery" => MAX_GALLERY,
            _ => 0,
        }
    }
    /// Sources whose actual size is known while parsing the UI document.
    pub fn static_source(&self, id: &str) -> Option<usize> {
        match id {
            "gallery" => Some(self.gallery),
            "variables" => Some(self.variables),
            "textLocales" => Some(self.text_locales),
            "voiceLocales" => Some(self.voice_locales),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub repeat: String,
    pub source: String,
    pub variable: String,
    pub page: i64,
    pub pages: i64,
    pub page_size: usize,
    pub total: usize,
    pub start: i64,
    pub end: i64,
}
impl Plan {
    pub fn can_prev(&self) -> bool {
        self.page > 1
    }
    pub fn can_next(&self) -> bool {
        self.page < self.pages
    }
    pub fn page_text(&self) -> String {
        crate::i18n::format(
            "host-interface-pagination-0001",
            &[
                ("arg0", self.page.to_string()),
                ("arg1", self.pages.to_string()),
            ],
        )
    }
    /// One-based inclusive source range for this page (empty when total is zero).
    pub fn range(&self) -> Option<(usize, usize)> {
        if self.total == 0 {
            return None;
        }
        Some((
            usize::try_from(self.start).unwrap_or(1),
            usize::try_from(self.end).unwrap_or(self.total),
        ))
    }
}

pub fn pages(total: usize, page_size: usize) -> i64 {
    let size = page_size.max(1) as u64;
    let total = total as u64;
    (total.div_ceil(size).max(1)) as i64
}

/// Compute the requested page from `variables`, clamping only natural source shrink.
pub fn clamp(variables: &Variables, variable: &str, total: usize, page_size: usize) -> i64 {
    let requested = match variables.get(variable) {
        Some(VariableValue::Int(value)) => *value,
        _ => 1,
    };
    requested.clamp(1, pages(total, page_size))
}

/// Every paginated repeat with its plan and normalized variable values.
pub fn plans(
    def: &Definition,
    variables: &Variables,
    totals: &Totals,
) -> (Vec<Plan>, BTreeMap<String, i64>) {
    let mut plans = Vec::new();
    let mut normalized = BTreeMap::new();
    for_each(def, |node| {
        let Kind::Repeat {
            source,
            pagination: Some(pagination),
            ..
        } = &node.kind
        else {
            return;
        };
        let total = totals.source(source);
        let page = clamp(variables, &pagination.variable, total, pagination.page_size);
        if variables.get(&pagination.variable) != Some(&VariableValue::Int(page)) {
            normalized.insert(pagination.variable.clone(), page);
        }
        let page_size = pagination.page_size;
        let count = pages(total, page_size);
        let offset = usize::try_from(page.saturating_sub(1))
            .unwrap_or(0)
            .saturating_mul(page_size);
        let (start, end) = if total == 0 {
            (0, 0)
        } else {
            (
                (offset + 1) as i64,
                (offset.saturating_add(page_size)).min(total) as i64,
            )
        };
        plans.push(Plan {
            repeat: node.id.clone(),
            source: source.clone(),
            variable: pagination.variable.clone(),
            page,
            pages: count,
            page_size,
            total,
            start,
            end,
        });
    });
    (plans, normalized)
}

/// One-based slot ranges needed for every explicitly paginated `slots` repeat.
pub fn slot_requests(def: &Definition, variables: &Variables) -> Vec<(String, u16, u16)> {
    let mut requests = Vec::new();
    for_each(def, |node| {
        let Kind::Repeat {
            source,
            pagination: Some(pagination),
            ..
        } = &node.kind
        else {
            return;
        };
        if source != "slots" {
            return;
        }
        let page = clamp(
            variables,
            &pagination.variable,
            MAX_SLOTS,
            pagination.page_size,
        );
        let offset = (page.saturating_sub(1) as usize).saturating_mul(pagination.page_size);
        if offset >= MAX_SLOTS {
            return;
        }
        let start = (offset + 1) as u16;
        let end = ((offset + pagination.page_size).min(MAX_SLOTS)) as u16;
        requests.push((node.id.clone(), start, end));
    });
    requests.sort();
    requests.dedup_by(|a, b| a.0 == b.0);
    requests
}

/// Walk every control of every screen in a stable order.
pub fn for_each(def: &Definition, mut visit: impl FnMut(&Node)) {
    fn walk(nodes: &[Node], visit: &mut impl FnMut(&Node)) {
        for node in nodes {
            visit(node);
            walk(&node.children, visit);
        }
    }
    for screen in def.screens.values() {
        walk(&screen.nodes, &mut visit);
    }
}

/// Stable per-repeat data key; never reuse the shared source list.
pub fn list_key(repeat: &str) -> String {
    format!("repeat:{repeat}")
}
