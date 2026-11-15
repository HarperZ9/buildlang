//! BuildOS GUI Theme System
//!
//! Theming support for customizing the look and feel of the GUI.

use super::Color;
use alloc::string::String;

/// Theme colors
#[derive(Clone, Debug)]
pub struct ThemeColors {
    // Primary colors
    pub primary: Color,
    pub primary_light: Color,
    pub primary_dark: Color,

    // Secondary colors
    pub secondary: Color,
    pub secondary_light: Color,
    pub secondary_dark: Color,

    // Background colors
    pub background: Color,
    pub surface: Color,
    pub card: Color,

    // Text colors
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_disabled: Color,
    pub text_hint: Color,

    // Semantic colors
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,

    // Border and divider
    pub border: Color,
    pub divider: Color,

    // Window decoration
    pub title_bar_active: Color,
    pub title_bar_inactive: Color,
    pub window_background: Color,
    pub window_border: Color,

    // Control colors
    pub button_background: Color,
    pub button_hover: Color,
    pub button_pressed: Color,
    pub button_disabled: Color,

    pub input_background: Color,
    pub input_border: Color,
    pub input_border_focused: Color,

    pub selection_background: Color,
    pub selection_text: Color,

    pub scrollbar_track: Color,
    pub scrollbar_thumb: Color,
    pub scrollbar_thumb_hover: Color,

    // Desktop
    pub desktop_background: Color,
    pub taskbar_background: Color,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self::light()
    }
}

impl ThemeColors {
    /// Light theme colors
    pub fn light() -> Self {
        Self {
            primary: Color::rgb(0, 120, 212),
            primary_light: Color::rgb(76, 166, 255),
            primary_dark: Color::rgb(0, 90, 158),

            secondary: Color::rgb(96, 96, 96),
            secondary_light: Color::rgb(150, 150, 150),
            secondary_dark: Color::rgb(60, 60, 60),

            background: Color::rgb(243, 243, 243),
            surface: Color::rgb(255, 255, 255),
            card: Color::rgb(255, 255, 255),

            text_primary: Color::rgb(32, 32, 32),
            text_secondary: Color::rgb(96, 96, 96),
            text_disabled: Color::rgb(160, 160, 160),
            text_hint: Color::rgb(128, 128, 128),

            success: Color::rgb(16, 124, 16),
            warning: Color::rgb(255, 185, 0),
            error: Color::rgb(232, 17, 35),
            info: Color::rgb(0, 120, 212),

            border: Color::rgb(200, 200, 200),
            divider: Color::rgb(220, 220, 220),

            title_bar_active: Color::rgb(0, 120, 212),
            title_bar_inactive: Color::rgb(200, 200, 200),
            window_background: Color::rgb(255, 255, 255),
            window_border: Color::rgb(180, 180, 180),

            button_background: Color::rgb(230, 230, 230),
            button_hover: Color::rgb(220, 220, 230),
            button_pressed: Color::rgb(200, 200, 220),
            button_disabled: Color::rgb(240, 240, 240),

            input_background: Color::rgb(255, 255, 255),
            input_border: Color::rgb(160, 160, 160),
            input_border_focused: Color::rgb(0, 120, 212),

            selection_background: Color::rgb(0, 120, 212),
            selection_text: Color::rgb(255, 255, 255),

            scrollbar_track: Color::rgb(240, 240, 240),
            scrollbar_thumb: Color::rgb(200, 200, 200),
            scrollbar_thumb_hover: Color::rgb(170, 170, 170),

            desktop_background: Color::rgb(0, 78, 152),
            taskbar_background: Color::rgb(32, 32, 32),
        }
    }

    /// Dark theme colors
    pub fn dark() -> Self {
        Self {
            primary: Color::rgb(76, 166, 255),
            primary_light: Color::rgb(130, 190, 255),
            primary_dark: Color::rgb(0, 120, 212),

            secondary: Color::rgb(150, 150, 150),
            secondary_light: Color::rgb(200, 200, 200),
            secondary_dark: Color::rgb(96, 96, 96),

            background: Color::rgb(32, 32, 32),
            surface: Color::rgb(45, 45, 45),
            card: Color::rgb(55, 55, 55),

            text_primary: Color::rgb(240, 240, 240),
            text_secondary: Color::rgb(180, 180, 180),
            text_disabled: Color::rgb(100, 100, 100),
            text_hint: Color::rgb(140, 140, 140),

            success: Color::rgb(56, 177, 56),
            warning: Color::rgb(255, 200, 50),
            error: Color::rgb(255, 69, 58),
            info: Color::rgb(76, 166, 255),

            border: Color::rgb(70, 70, 70),
            divider: Color::rgb(60, 60, 60),

            title_bar_active: Color::rgb(45, 45, 45),
            title_bar_inactive: Color::rgb(55, 55, 55),
            window_background: Color::rgb(40, 40, 40),
            window_border: Color::rgb(60, 60, 60),

            button_background: Color::rgb(60, 60, 60),
            button_hover: Color::rgb(70, 70, 80),
            button_pressed: Color::rgb(50, 50, 60),
            button_disabled: Color::rgb(50, 50, 50),

            input_background: Color::rgb(50, 50, 50),
            input_border: Color::rgb(80, 80, 80),
            input_border_focused: Color::rgb(76, 166, 255),

            selection_background: Color::rgb(76, 166, 255),
            selection_text: Color::rgb(0, 0, 0),

            scrollbar_track: Color::rgb(45, 45, 45),
            scrollbar_thumb: Color::rgb(80, 80, 80),
            scrollbar_thumb_hover: Color::rgb(100, 100, 100),

            desktop_background: Color::rgb(20, 20, 30),
            taskbar_background: Color::rgb(24, 24, 24),
        }
    }

    /// High contrast theme
    pub fn high_contrast() -> Self {
        Self {
            primary: Color::rgb(0, 255, 255),
            primary_light: Color::rgb(100, 255, 255),
            primary_dark: Color::rgb(0, 200, 200),

            secondary: Color::rgb(255, 255, 0),
            secondary_light: Color::rgb(255, 255, 100),
            secondary_dark: Color::rgb(200, 200, 0),

            background: Color::rgb(0, 0, 0),
            surface: Color::rgb(0, 0, 0),
            card: Color::rgb(0, 0, 0),

            text_primary: Color::rgb(255, 255, 255),
            text_secondary: Color::rgb(255, 255, 255),
            text_disabled: Color::rgb(128, 128, 128),
            text_hint: Color::rgb(200, 200, 200),

            success: Color::rgb(0, 255, 0),
            warning: Color::rgb(255, 255, 0),
            error: Color::rgb(255, 0, 0),
            info: Color::rgb(0, 255, 255),

            border: Color::rgb(255, 255, 255),
            divider: Color::rgb(255, 255, 255),

            title_bar_active: Color::rgb(0, 0, 128),
            title_bar_inactive: Color::rgb(0, 0, 0),
            window_background: Color::rgb(0, 0, 0),
            window_border: Color::rgb(255, 255, 255),

            button_background: Color::rgb(0, 0, 0),
            button_hover: Color::rgb(0, 0, 128),
            button_pressed: Color::rgb(0, 128, 128),
            button_disabled: Color::rgb(32, 32, 32),

            input_background: Color::rgb(0, 0, 0),
            input_border: Color::rgb(255, 255, 255),
            input_border_focused: Color::rgb(0, 255, 255),

            selection_background: Color::rgb(255, 255, 255),
            selection_text: Color::rgb(0, 0, 0),

            scrollbar_track: Color::rgb(0, 0, 0),
            scrollbar_thumb: Color::rgb(255, 255, 255),
            scrollbar_thumb_hover: Color::rgb(0, 255, 255),

            desktop_background: Color::rgb(0, 0, 0),
            taskbar_background: Color::rgb(0, 0, 0),
        }
    }
}

/// Font settings
#[derive(Clone, Debug)]
pub struct ThemeFonts {
    /// Default font family
    pub family: String,
    /// Heading font family
    pub heading_family: String,
    /// Monospace font family
    pub monospace_family: String,

    /// Base font size
    pub size_base: u8,
    /// Small font size
    pub size_small: u8,
    /// Large font size
    pub size_large: u8,
    /// Heading font size
    pub size_heading: u8,

    /// Line height multiplier (x100)
    pub line_height: u16,
}

impl Default for ThemeFonts {
    fn default() -> Self {
        Self {
            family: String::from("System"),
            heading_family: String::from("System"),
            monospace_family: String::from("Monospace"),
            size_base: 14,
            size_small: 12,
            size_large: 16,
            size_heading: 20,
            line_height: 140, // 1.4x
        }
    }
}

/// Spacing and sizing
#[derive(Clone, Debug)]
pub struct ThemeSpacing {
    /// Base spacing unit
    pub unit: u32,
    /// Extra small spacing (unit / 2)
    pub xs: u32,
    /// Small spacing (unit)
    pub sm: u32,
    /// Medium spacing (unit * 2)
    pub md: u32,
    /// Large spacing (unit * 3)
    pub lg: u32,
    /// Extra large spacing (unit * 4)
    pub xl: u32,

    /// Default padding
    pub padding: u32,
    /// Default margin
    pub margin: u32,

    /// Border radius
    pub border_radius: u32,
    /// Border width
    pub border_width: u32,

    /// Control height (small)
    pub control_height_sm: u32,
    /// Control height (normal)
    pub control_height: u32,
    /// Control height (large)
    pub control_height_lg: u32,

    /// Icon size (small)
    pub icon_size_sm: u32,
    /// Icon size (normal)
    pub icon_size: u32,
    /// Icon size (large)
    pub icon_size_lg: u32,
}

impl Default for ThemeSpacing {
    fn default() -> Self {
        let unit = 8;
        Self {
            unit,
            xs: unit / 2,
            sm: unit,
            md: unit * 2,
            lg: unit * 3,
            xl: unit * 4,
            padding: unit,
            margin: unit,
            border_radius: 4,
            border_width: 1,
            control_height_sm: 24,
            control_height: 32,
            control_height_lg: 40,
            icon_size_sm: 16,
            icon_size: 24,
            icon_size_lg: 32,
        }
    }
}

/// Animation settings
#[derive(Clone, Debug)]
pub struct ThemeAnimations {
    /// Duration for fast animations (ms)
    pub duration_fast: u32,
    /// Duration for normal animations (ms)
    pub duration_normal: u32,
    /// Duration for slow animations (ms)
    pub duration_slow: u32,

    /// Enable animations
    pub enabled: bool,
    /// Reduce motion for accessibility
    pub reduce_motion: bool,
}

impl Default for ThemeAnimations {
    fn default() -> Self {
        Self {
            duration_fast: 100,
            duration_normal: 200,
            duration_slow: 300,
            enabled: true,
            reduce_motion: false,
        }
    }
}

/// Shadow settings
#[derive(Clone, Debug)]
pub struct Shadow {
    pub offset_x: i32,
    pub offset_y: i32,
    pub blur: u32,
    pub spread: i32,
    pub color: Color,
}

impl Shadow {
    pub fn none() -> Self {
        Self {
            offset_x: 0,
            offset_y: 0,
            blur: 0,
            spread: 0,
            color: Color::TRANSPARENT,
        }
    }

    pub fn small() -> Self {
        Self {
            offset_x: 0,
            offset_y: 2,
            blur: 4,
            spread: 0,
            color: Color::rgba(0, 0, 0, 32),
        }
    }

    pub fn medium() -> Self {
        Self {
            offset_x: 0,
            offset_y: 4,
            blur: 8,
            spread: 0,
            color: Color::rgba(0, 0, 0, 48),
        }
    }

    pub fn large() -> Self {
        Self {
            offset_x: 0,
            offset_y: 8,
            blur: 16,
            spread: 0,
            color: Color::rgba(0, 0, 0, 64),
        }
    }
}

/// Theme shadows
#[derive(Clone, Debug)]
pub struct ThemeShadows {
    pub none: Shadow,
    pub sm: Shadow,
    pub md: Shadow,
    pub lg: Shadow,
    pub window: Shadow,
    pub popup: Shadow,
    pub tooltip: Shadow,
}

impl Default for ThemeShadows {
    fn default() -> Self {
        Self {
            none: Shadow::none(),
            sm: Shadow::small(),
            md: Shadow::medium(),
            lg: Shadow::large(),
            window: Shadow {
                offset_x: 0,
                offset_y: 4,
                blur: 12,
                spread: 0,
                color: Color::rgba(0, 0, 0, 80),
            },
            popup: Shadow {
                offset_x: 0,
                offset_y: 2,
                blur: 8,
                spread: 0,
                color: Color::rgba(0, 0, 0, 48),
            },
            tooltip: Shadow {
                offset_x: 0,
                offset_y: 1,
                blur: 4,
                spread: 0,
                color: Color::rgba(0, 0, 0, 32),
            },
        }
    }
}

/// Complete theme
#[derive(Clone, Debug)]
pub struct Theme {
    /// Theme name
    pub name: String,
    /// Theme colors
    pub colors: ThemeColors,
    /// Theme fonts
    pub fonts: ThemeFonts,
    /// Theme spacing
    pub spacing: ThemeSpacing,
    /// Theme animations
    pub animations: ThemeAnimations,
    /// Theme shadows
    pub shadows: ThemeShadows,
    /// Is dark theme
    pub is_dark: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
    /// Create light theme
    pub fn light() -> Self {
        Self {
            name: String::from("Light"),
            colors: ThemeColors::light(),
            fonts: ThemeFonts::default(),
            spacing: ThemeSpacing::default(),
            animations: ThemeAnimations::default(),
            shadows: ThemeShadows::default(),
            is_dark: false,
        }
    }

    /// Create dark theme
    pub fn dark() -> Self {
        Self {
            name: String::from("Dark"),
            colors: ThemeColors::dark(),
            fonts: ThemeFonts::default(),
            spacing: ThemeSpacing::default(),
            animations: ThemeAnimations::default(),
            shadows: ThemeShadows::default(),
            is_dark: true,
        }
    }

    /// Create high contrast theme
    pub fn high_contrast() -> Self {
        Self {
            name: String::from("High Contrast"),
            colors: ThemeColors::high_contrast(),
            fonts: ThemeFonts::default(),
            spacing: ThemeSpacing::default(),
            animations: ThemeAnimations {
                enabled: false,
                ..Default::default()
            },
            shadows: ThemeShadows {
                none: Shadow::none(),
                sm: Shadow::none(),
                md: Shadow::none(),
                lg: Shadow::none(),
                window: Shadow::none(),
                popup: Shadow::none(),
                tooltip: Shadow::none(),
            },
            is_dark: true,
        }
    }
}

/// Global theme state
use crate::sync::RwLock;
static CURRENT_THEME: RwLock<Option<Theme>> = RwLock::new(None);

/// Initialize theme system with default theme
pub fn init() {
    set_theme(Theme::light());
}

/// Set current theme
pub fn set_theme(theme: Theme) {
    let mut current = CURRENT_THEME.write();
    *current = Some(theme);
}

/// Get current theme
pub fn get_theme() -> Theme {
    CURRENT_THEME.read().clone().unwrap_or_default()
}

/// Get current colors
pub fn colors() -> ThemeColors {
    get_theme().colors
}

/// Get current spacing
pub fn spacing() -> ThemeSpacing {
    get_theme().spacing
}

/// Toggle between light and dark themes
pub fn toggle_dark_mode() {
    let current = get_theme();
    if current.is_dark {
        set_theme(Theme::light());
    } else {
        set_theme(Theme::dark());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_light_theme() {
        let theme = Theme::light();
        assert_eq!(theme.name, "Light");
        assert!(!theme.is_dark);
    }

    #[test]
    fn test_dark_theme() {
        let theme = Theme::dark();
        assert_eq!(theme.name, "Dark");
        assert!(theme.is_dark);
    }

    #[test]
    fn test_shadow() {
        let shadow = Shadow::medium();
        assert_eq!(shadow.blur, 8);
    }
}
