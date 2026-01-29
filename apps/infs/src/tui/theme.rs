//! TUI theme system.
//!
//! This module provides a simple theme system for consistent styling
//! across the TUI application. Currently only a dark theme is supported.

use ratatui::style::Color;

/// Theme colors for the TUI application.
///
/// Provides a consistent color palette for all TUI elements.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Color for highlighted/active elements.
    pub highlight: Color,
    /// Color for selected items in lists.
    pub selected: Color,
    /// Color for borders.
    pub border: Color,
    /// Color for success indicators.
    pub success: Color,
    /// Color for warning indicators.
    pub warning: Color,
    /// Color for error indicators.
    pub error: Color,
    /// Color for muted/secondary text.
    pub muted: Color,
    /// Color for primary text.
    pub text: Color,
    /// Background color for selected items.
    #[allow(dead_code)]
    pub selected_bg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// Creates a dark theme.
    ///
    /// This is the default theme for dark terminal backgrounds.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            highlight: Color::Cyan,
            selected: Color::LightBlue,
            border: Color::DarkGray,
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            muted: Color::DarkGray,
            text: Color::White,
            selected_bg: Color::DarkGray,
        }
    }

    /// Creates a light theme.
    ///
    /// This theme is designed for light terminal backgrounds.
    #[must_use]
    pub fn light() -> Self {
        Self {
            highlight: Color::Blue,
            selected: Color::DarkGray,
            border: Color::Gray,
            success: Color::Rgb(0, 128, 0),   // Dark green
            warning: Color::Rgb(204, 153, 0), // Dark yellow/gold
            error: Color::Rgb(139, 0, 0),     // Dark red
            muted: Color::Gray,
            text: Color::Black,
            selected_bg: Color::LightYellow,
        }
    }

    /// Detects the appropriate theme based on the COLORFGBG environment variable.
    ///
    /// The COLORFGBG format is "foreground;background" where both are ANSI color
    /// codes (0-15). Background colors 0-7 are typically dark, 8-15 are typically
    /// light. If detection fails, defaults to dark theme.
    ///
    /// # Examples
    ///
    /// - `COLORFGBG=15;0` - White on black (dark theme)
    /// - `COLORFGBG=0;15` - Black on white (light theme)
    /// - `COLORFGBG=default;default` - Unset or default (dark theme)
    #[must_use]
    pub fn detect() -> Self {
        detect_theme_from_env().unwrap_or_else(Self::dark)
    }
}

/// Attempts to detect the theme from the COLORFGBG environment variable.
fn detect_theme_from_env() -> Option<Theme> {
    let colorfgbg = std::env::var("COLORFGBG").ok()?;
    detect_theme_from_colorfgbg(&colorfgbg)
}

/// Parses the COLORFGBG value and returns the appropriate theme.
///
/// Returns `None` if the format is invalid or background color cannot be determined.
fn detect_theme_from_colorfgbg(value: &str) -> Option<Theme> {
    // Format: "foreground;background" or "foreground;background;..."
    let parts: Vec<&str> = value.split(';').collect();

    if parts.len() < 2 {
        return None;
    }

    // Try to parse the background color (second value)
    let bg_str = parts[1].trim();

    // Handle "default" or non-numeric values
    let bg_color: u8 = bg_str.parse().ok()?;

    // ANSI colors 0-7 are dark colors, 8-15 are light colors
    // 0: black, 1: red, 2: green, 3: yellow, 4: blue, 5: magenta, 6: cyan, 7: white (light gray)
    // 8-15: bright versions of the above
    //
    // Typically:
    // - bg < 8: dark background (except 7 which is light gray)
    // - bg >= 8: light background
    // - bg == 7: light gray, often used as light background
    if bg_color >= 8 || bg_color == 7 {
        Some(Theme::light())
    } else {
        Some(Theme::dark())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_default_is_dark() {
        let default = Theme::default();
        let dark = Theme::dark();
        assert_eq!(default.highlight, dark.highlight);
        assert_eq!(default.selected, dark.selected);
        assert_eq!(default.border, dark.border);
        assert_eq!(default.success, dark.success);
        assert_eq!(default.warning, dark.warning);
        assert_eq!(default.error, dark.error);
        assert_eq!(default.muted, dark.muted);
    }

    #[test]
    fn dark_theme_has_expected_colors() {
        let theme = Theme::dark();
        assert_eq!(theme.highlight, Color::Cyan);
        assert_eq!(theme.success, Color::Green);
        assert_eq!(theme.warning, Color::Yellow);
        assert_eq!(theme.error, Color::Red);
    }

    #[test]
    fn light_theme_has_expected_colors() {
        let theme = Theme::light();
        assert_eq!(theme.highlight, Color::Blue);
        assert_eq!(theme.success, Color::Rgb(0, 128, 0)); // Dark green
        assert_eq!(theme.error, Color::Rgb(139, 0, 0)); // Dark red
        assert_eq!(theme.text, Color::Black);
    }

    #[test]
    fn detect_colorfgbg_dark_background() {
        // Black background (color 0)
        assert!(detect_theme_from_colorfgbg("15;0").is_some());
        let theme = detect_theme_from_colorfgbg("15;0").unwrap();
        assert_eq!(theme.text, Color::White); // Dark theme

        // Blue background (color 4)
        let theme = detect_theme_from_colorfgbg("7;4").unwrap();
        assert_eq!(theme.text, Color::White); // Dark theme
    }

    #[test]
    fn detect_colorfgbg_light_background() {
        // White/light gray background (color 7)
        let theme = detect_theme_from_colorfgbg("0;7").unwrap();
        assert_eq!(theme.text, Color::Black); // Light theme

        // Bright white background (color 15)
        let theme = detect_theme_from_colorfgbg("0;15").unwrap();
        assert_eq!(theme.text, Color::Black); // Light theme

        // Bright cyan background (color 14)
        let theme = detect_theme_from_colorfgbg("0;14").unwrap();
        assert_eq!(theme.text, Color::Black); // Light theme
    }

    #[test]
    fn detect_colorfgbg_invalid_format() {
        assert!(detect_theme_from_colorfgbg("").is_none());
        assert!(detect_theme_from_colorfgbg("15").is_none());
        assert!(detect_theme_from_colorfgbg("default;default").is_none());
        assert!(detect_theme_from_colorfgbg("abc;xyz").is_none());
    }

    #[test]
    fn detect_colorfgbg_with_extra_parts() {
        // Some terminals use format "fg;bg;extra"
        let theme = detect_theme_from_colorfgbg("15;0;extra").unwrap();
        assert_eq!(theme.text, Color::White); // Dark theme
    }

    #[test]
    fn detect_returns_dark_on_failure() {
        // When no COLORFGBG is set (or invalid), detect() should return dark theme
        let theme = Theme::detect();
        // We can only verify it returns a valid theme
        let _ = theme.highlight;
    }
}
