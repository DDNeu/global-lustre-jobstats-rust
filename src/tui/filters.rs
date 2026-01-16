//! Filters for job ID and operation types

use std::collections::{HashMap, HashSet};

use crate::op_keys::op_long_keys;

/// Filter for operation types
#[derive(Debug, Clone)]
pub struct OperationFilter {
    /// Set of enabled operation types
    enabled: HashSet<String>,
    /// All available operation types (for UI display)
    all_ops: Vec<String>,
    /// Currently selected index (for UI navigation)
    pub selected_idx: usize,
}

impl Default for OperationFilter {
    fn default() -> Self {
        // Default to showing read, write, open, close, getattr
        let default_enabled: HashSet<String> = [
            "read", "write", "open", "close", "getattr", "setattr",
            "punch", "read_bytes", "write_bytes",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let all_ops: Vec<String> = op_long_keys().iter().map(|s| s.to_string()).collect();

        Self {
            enabled: default_enabled,
            all_ops,
            selected_idx: 0,
        }
    }
}

impl OperationFilter {
    /// Check if an operation is enabled
    pub fn is_enabled(&self, op: &str) -> bool {
        self.enabled.contains(op)
    }

    /// Toggle an operation
    pub fn toggle(&mut self, op: &str) {
        if self.enabled.contains(op) {
            self.enabled.remove(op);
        } else {
            self.enabled.insert(op.to_string());
        }
    }

    /// Toggle currently selected operation
    pub fn toggle_selected(&mut self) {
        if let Some(op) = self.all_ops.get(self.selected_idx) {
            let op = op.clone();
            self.toggle(&op);
        }
    }

    /// Get all operations with their enabled state
    pub fn all_with_state(&self) -> Vec<(&str, bool)> {
        self.all_ops
            .iter()
            .map(|op| (op.as_str(), self.enabled.contains(op)))
            .collect()
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if self.selected_idx > 0 {
            self.selected_idx -= 1;
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.selected_idx < self.all_ops.len() - 1 {
            self.selected_idx += 1;
        }
    }

    /// Enable all operations
    pub fn enable_all(&mut self) {
        self.enabled = self.all_ops.iter().cloned().collect();
    }

    /// Disable all operations
    pub fn disable_all(&mut self) {
        self.enabled.clear();
    }

    /// Get enabled operations as slice
    pub fn enabled_ops(&self) -> Vec<&str> {
        self.enabled.iter().map(|s| s.as_str()).collect()
    }
}

/// Filter for job IDs (full match or component-based)
#[derive(Debug, Clone)]
pub struct JobIdFilter {
    /// Full job_id substring filter
    pub full_filter: String,
    /// Component-based filters (component name -> filter value)
    pub component_filters: HashMap<String, String>,
    /// Component names in order
    pub component_names: Vec<String>,
    /// Separator character
    pub separator: char,
    /// Currently selected component index (for UI navigation)
    pub selected_component: usize,
    /// Whether editing full filter (true) or component (false)
    pub editing_full: bool,
}

impl JobIdFilter {
    pub fn new(component_names: Vec<String>, separator: char) -> Self {
        let component_filters: HashMap<String, String> = component_names
            .iter()
            .map(|name| (name.clone(), String::new()))
            .collect();

        Self {
            full_filter: String::new(),
            component_filters,
            component_names,
            separator,
            selected_component: 0,
            editing_full: true,
        }
    }

    /// Check if a job_id matches the current filters
    pub fn matches(&self, job_id: &str) -> bool {
        // Check full filter first
        if !self.full_filter.is_empty() && !job_id.contains(&self.full_filter) {
            return false;
        }

        // Check component filters
        let parts: Vec<&str> = job_id.split(self.separator).collect();

        for (i, name) in self.component_names.iter().enumerate() {
            if let Some(filter) = self.component_filters.get(name) {
                if !filter.is_empty() {
                    if let Some(part) = parts.get(i) {
                        if !part.contains(filter.as_str()) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Clear all filters
    pub fn clear(&mut self) {
        self.full_filter.clear();
        for v in self.component_filters.values_mut() {
            v.clear();
        }
    }

    /// Get current filter value for editing
    #[allow(dead_code)]
    pub fn current_filter_value(&self) -> &str {
        if self.editing_full {
            &self.full_filter
        } else if let Some(name) = self.component_names.get(self.selected_component) {
            self.component_filters.get(name).map(|s| s.as_str()).unwrap_or("")
        } else {
            ""
        }
    }

    /// Append character to current filter
    pub fn push_char(&mut self, c: char) {
        if self.editing_full {
            self.full_filter.push(c);
        } else if let Some(name) = self.component_names.get(self.selected_component) {
            if let Some(v) = self.component_filters.get_mut(name) {
                v.push(c);
            }
        }
    }

    /// Remove last character from current filter
    pub fn pop_char(&mut self) {
        if self.editing_full {
            self.full_filter.pop();
        } else if let Some(name) = self.component_names.get(self.selected_component) {
            if let Some(v) = self.component_filters.get_mut(name) {
                v.pop();
            }
        }
    }

    /// Move to next component
    pub fn next_component(&mut self) {
        if self.editing_full {
            self.editing_full = false;
            self.selected_component = 0;
        } else if self.selected_component < self.component_names.len() - 1 {
            self.selected_component += 1;
        } else {
            self.editing_full = true;
        }
    }

    /// Move to previous component
    pub fn prev_component(&mut self) {
        if self.editing_full {
            if !self.component_names.is_empty() {
                self.editing_full = false;
                self.selected_component = self.component_names.len() - 1;
            }
        } else if self.selected_component > 0 {
            self.selected_component -= 1;
        } else {
            self.editing_full = true;
        }
    }
}

