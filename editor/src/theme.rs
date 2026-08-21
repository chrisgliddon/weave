//! Shared visual tokens for every editor surface.

/// Complete editor theme expressed as renderer-independent tokens.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorTheme {
    /// Main application background.
    pub workspace: u32,
    /// Elevated panel background.
    pub panel: u32,
    /// Toolbar and titlebar background.
    pub chrome: u32,
    /// Hairline and control border color.
    pub border: u32,
    /// Primary text color.
    pub text: u32,
    /// Secondary text color.
    pub muted_text: u32,
    /// Active selection and focus color.
    pub accent: u32,
    /// Error surface color.
    pub error: u32,
    /// Warning surface color.
    pub warning: u32,
    /// Success surface color.
    pub success: u32,
    /// Base spacing unit in logical pixels.
    pub spacing: f32,
    /// Standard control corner radius in logical pixels.
    pub radius: f32,
}

/// Default dark editor theme.
pub const DARK_THEME: EditorTheme = EditorTheme {
    workspace: 0x0b0d12,
    panel: 0x121620,
    chrome: 0x171c27,
    border: 0x2a3242,
    text: 0xf4f6fb,
    muted_text: 0x99a3b5,
    accent: 0x7c9cff,
    error: 0x7f1d2d,
    warning: 0x7c4a03,
    success: 0x155e45,
    spacing: 8.0,
    radius: 6.0,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_theme_has_distinct_focus_and_error_tokens() {
        assert_ne!(DARK_THEME.workspace, DARK_THEME.panel);
        assert_ne!(DARK_THEME.accent, DARK_THEME.error);
        const {
            assert!(DARK_THEME.spacing > 0.0);
            assert!(DARK_THEME.radius > 0.0);
        }
    }
}
