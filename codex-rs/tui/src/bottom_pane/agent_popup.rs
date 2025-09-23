use codex_core::protocol::AgentInfo;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::WidgetRef;

use super::popup_consts::MAX_POPUP_ROWS;
use super::scroll_state::ScrollState;
use super::selection_popup_common::GenericDisplayRow;
use super::selection_popup_common::measure_rows_height;
use super::selection_popup_common::render_rows;

/// Visual state for the agent-suggestion popup.
pub(crate) struct AgentPopup {
    /// Last query used to compute matches
    query: String,
    /// Filtered agents rendered as rows
    rows: Vec<GenericDisplayRow>,
    /// Shared selection/scroll state.
    state: ScrollState,
}

impl AgentPopup {
    pub(crate) fn new() -> Self {
        Self {
            query: String::new(),
            rows: Vec::new(),
            state: ScrollState::new(),
        }
    }

    /// Update the query and compute matches from `agents`.
    pub(crate) fn set_query(&mut self, query: &str, agents: &[AgentInfo]) {
        if self.query == query {
            return;
        }
        self.query.clear();
        self.query.push_str(query);

        // Accept both "agent", "agent:", and "agent " prefixes to list all agents.
        // If there is additional text after the prefix, filter by that remainder.
        let q_lower = query.to_lowercase();
        let remainder = if let Some(rest) = q_lower.strip_prefix("agent") {
            let rest = rest.trim_start_matches([':', ' ']);
            Some(rest)
        } else {
            None
        };

        let mut rows: Vec<GenericDisplayRow> = match remainder {
            // No remainder or empty remainder => show all agents
            Some("") | None => agents
                .iter()
                .map(|a| GenericDisplayRow {
                    name: a.name.clone(),
                    match_indices: None,
                    is_current: false,
                    description: Some(a.description.clone()),
                })
                .collect(),
            // Filter by the remainder after the literal "agent" prefix
            Some(rem) => {
                let needle = rem.to_string();
                agents
                    .iter()
                    .filter(|a| a.name.to_lowercase().contains(&needle))
                    .map(|a| GenericDisplayRow {
                        name: a.name.clone(),
                        match_indices: highlight_indices(&a.name, &needle),
                        is_current: false,
                        description: Some(a.description.clone()),
                    })
                    .collect()
            }
        };

        // Stable alphabetical ordering
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        self.rows = rows;
        self.state.clamp_selection(self.rows.len());
        self.state.ensure_visible(self.rows.len(), self.rows.len().min(MAX_POPUP_ROWS));
    }

    pub(crate) fn move_up(&mut self) {
        let len = self.rows.len();
        self.state.move_up_wrap(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }

    pub(crate) fn move_down(&mut self) {
        let len = self.rows.len();
        self.state.move_down_wrap(len);
        self.state.ensure_visible(len, len.min(MAX_POPUP_ROWS));
    }

    pub(crate) fn selected_agent(&self) -> Option<&str> {
        self.state
            .selected_idx
            .and_then(|idx| self.rows.get(idx))
            .map(|row| row.name.as_str())
    }

    pub(crate) fn calculate_required_height(&self, width: u16) -> u16 {
        measure_rows_height(&self.rows, &self.state, MAX_POPUP_ROWS, width)
    }
}

fn highlight_indices(name: &str, query_lower: &str) -> Option<Vec<usize>> {
    if query_lower.is_empty() {
        return None;
    }
    let name_lower = name.to_lowercase();
    let mut indices = Vec::new();
    let mut i_name = 0usize;
    for ch in name_lower.chars() {
        if query_lower.contains(ch) {
            indices.push(i_name);
        }
        i_name += 1;
    }
    if indices.is_empty() {
        None
    } else {
        Some(indices)
    }
}

impl WidgetRef for AgentPopup {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        render_rows(
            area,
            buf,
            &self.rows,
            &self.state,
            MAX_POPUP_ROWS,
            false,
            if self.query.is_empty() { "type an agent name" } else { "no agents" },
        );
    }
}
