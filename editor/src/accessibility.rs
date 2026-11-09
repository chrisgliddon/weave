//! Toolkit-independent accessibility tree shared by editor surfaces.
//!
//! The pinned `gpui` revision exposes no accessibility API, so the editor
//! publishes a logical tree instead. A host with platform accessibility support
//! can attach these nodes to its own accessibility bridge without changing the
//! surface logic.

/// One node in a logical accessibility tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessibleNode {
    /// Platform-neutral role, such as `button`, `tab`, `option`, or `group`.
    pub role: &'static str,
    /// Author-facing label read by assistive technology.
    pub label: String,
    /// Whether this node currently holds focus.
    pub focused: bool,
    /// Whether the control is present but unavailable.
    pub disabled: bool,
}

impl AccessibleNode {
    /// Build one node.
    #[must_use]
    pub fn new(role: &'static str, label: impl Into<String>) -> Self {
        Self {
            role,
            label: label.into(),
            focused: false,
            disabled: false,
        }
    }

    /// Mark this node as focused.
    #[must_use]
    pub const fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Mark this node as unavailable.
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// True when exactly one node carries focus, or the tree is empty.
#[must_use]
pub fn focus_is_unique(nodes: &[AccessibleNode]) -> bool {
    nodes.iter().filter(|node| node.focused).count() <= 1
}
