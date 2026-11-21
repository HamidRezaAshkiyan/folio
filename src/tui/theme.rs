use crate::core::model::Signifier;
use ratatui::style::Color;

/// Exact Folio dark-mode design tokens from the HTML mockup.
pub const SURFACE: Color = Color::Rgb(0x15, 0x13, 0x0E); // #15130E
pub const SURFACE_LOWEST: Color = Color::Rgb(0x10, 0x0E, 0x09); // #100E09
pub const SURFACE_LOW: Color = Color::Rgb(0x1E, 0x1B, 0x16); // #1E1B16
pub const SURFACE_CONTAINER: Color = Color::Rgb(0x22, 0x1F, 0x1A); // #221F1A
pub const SURFACE_HIGH: Color = Color::Rgb(0x2C, 0x2A, 0x24); // #2C2A24
pub const OUTLINE_VARIANT: Color = Color::Rgb(0x44, 0x47, 0x4E); // #44474E

pub const PRIMARY: Color = Color::Rgb(0xB1, 0xC7, 0xF1); // #B1C7F1
pub const ON_PRIMARY: Color = Color::Rgb(0x1A, 0x31, 0x52); // #1A3152
pub const SECONDARY: Color = Color::Rgb(0xFF, 0xB4, 0xA8); // #FFB4A8
pub const TERTIARY: Color = Color::Rgb(0xE5, 0xC2, 0x81); // #E5C281

pub const ON_SURFACE: Color = Color::Rgb(0xE8, 0xE2, 0xD9); // #E8E2D9
pub const ON_SURFACE_VARIANT: Color = Color::Rgb(0xC4, 0xC6, 0xCF); // #C4C6CF

pub fn signifier_color(sig: Signifier) -> Color {
    match sig {
        Signifier::Action => PRIMARY,
        Signifier::Done | Signifier::Cancelled => ON_SURFACE_VARIANT,
        Signifier::Migrated | Signifier::Scheduled => PRIMARY,
        Signifier::Event => TERTIARY,
        Signifier::Emotion => SECONDARY,
        Signifier::Note => ON_SURFACE_VARIANT,
    }
}
