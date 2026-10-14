//! Shared text roles, native metrics and font palette for the game interface.
//! Native sizes come from the player's sheets, never from the length of a string.

use macroquad::prelude::Color;
use super::dt_font::{self, Face};

#[derive(Clone, Copy)]
pub enum Role { Body, Bold, Title, Name, Button, Hint }

impl Role {
    pub fn face(self) -> Face {
        match self {
            Self::Body | Self::Hint => Face::Body,
            Self::Bold => Face::Bold,
            Self::Title => Face::Title,
            Self::Name | Self::Button => Face::Subtitle,
        }
    }

    pub fn with<R>(self, f: impl FnOnce() -> R) -> R { dt_font::with_face(self.face(), f) }
    pub fn size(self, fallback: f32) -> f32 { dt_font::native_size(self.face(), fallback) }
    pub fn line_height(self, fallback: f32) -> f32 { dt_font::native_line_height(self.face(), fallback) }
}

/// A bitmap font exists and is being laid out at its native interface scale.
pub fn fixed_size() -> bool {
    dt_font::native_metrics() && dt_font::with_current(|_| ()).is_some()
}

/// Startup font variants in the original (4dbb0c), by their semantic use.
#[derive(Clone, Copy)]
pub enum Ink { Plain, Yellow, Blue, Red, Orange, Muted, Green }

impl Ink {
    pub fn color(self) -> Color {
        let rgb = match self {
            Self::Plain => [255, 255, 255],
            Self::Yellow => [255, 255, 155],
            Self::Blue if matches!(dt_font::face(), Face::Body | Face::Bold) => [75, 185, 255],
            Self::Blue => [55, 180, 255],
            Self::Red => [255, 103, 96],
            Self::Orange if matches!(dt_font::face(), Face::Body | Face::Bold) => [235, 165, 75],
            Self::Orange => [255, 175, 85],
            Self::Muted => [155, 155, 155],
            Self::Green => [155, 255, 155],
        };
        Color::from_rgba(rgb[0], rgb[1], rgb[2], 255)
    }
}

/// Resolve shared legacy text colours, leaving screen-specific RGB choices intact.
pub fn color(c: Color) -> Color {
    if !dt_font::native_metrics() { return c; }
    let rgb = Color { a: 1.0, ..c };
    let ink = if rgb == super::chrome::CREAM || rgb == super::widgets::INK { Some(Ink::Plain) }
        else if rgb == super::chrome::GOLD || rgb == super::widgets::ACCENT { Some(Ink::Yellow) }
        else if rgb == super::chrome::BLUE_TEXT { Some(Ink::Blue) }
        else if rgb == super::chrome::RED_TEXT { Some(Ink::Red) }
        else if rgb == super::chrome::ORANGE_TEXT { Some(Ink::Orange) }
        else if rgb == super::widgets::DIM { Some(Ink::Muted) }
        else { None };
    ink.map_or(c, |i| Color { a: c.a, ..i.color() })
}
