use std::sync::LazyLock;

use ratatui::style::Color;

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub bg: Color,
    pub fg: Color,
    pub cursor: Color,
    pub correct: Color,
    pub incorrect: Color,
    pub subtle: Color,
    pub accent: Color,
    pub border: Color,
}

static PALETTE: LazyLock<Vec<Theme>> = LazyLock::new(|| {
    vec![
        Theme::catppuccin_mocha(),
        Theme::dracula(),
        Theme::gruvbox_dark(),
        Theme::tokyo_night(),
        Theme::nord(),
        Theme::monokai(),
    ]
});

impl Theme {
    pub fn all() -> &'static [Theme] {
        PALETTE.as_slice()
    }

    pub fn by_name(name: &str) -> Option<&'static Theme> {
        Self::all().iter().find(|theme| theme.name == name)
    }

    pub fn names() -> Vec<String> {
        Self::all().iter().map(|theme| theme.name.clone()).collect()
    }

    pub fn catppuccin_mocha() -> Theme {
        Theme {
            name: "catppuccin_mocha".to_string(),
            bg: Color::Rgb(30, 30, 46),
            fg: Color::Rgb(205, 214, 244),
            cursor: Color::Rgb(245, 224, 220),
            correct: Color::Rgb(166, 227, 161),
            incorrect: Color::Rgb(243, 139, 168),
            subtle: Color::Rgb(166, 173, 200),
            accent: Color::Rgb(137, 180, 250),
            border: Color::Rgb(88, 91, 112),
        }
    }

    pub fn dracula() -> Theme {
        Theme {
            name: "dracula".to_string(),
            bg: Color::Rgb(40, 42, 54),
            fg: Color::Rgb(248, 248, 242),
            cursor: Color::Rgb(255, 121, 198),
            correct: Color::Rgb(80, 250, 123),
            incorrect: Color::Rgb(255, 85, 85),
            subtle: Color::Rgb(189, 189, 199),
            accent: Color::Rgb(189, 147, 249),
            border: Color::Rgb(68, 71, 90),
        }
    }

    pub fn gruvbox_dark() -> Theme {
        Theme {
            name: "gruvbox_dark".to_string(),
            bg: Color::Rgb(29, 32, 33),
            fg: Color::Rgb(235, 219, 178),
            cursor: Color::Rgb(250, 189, 47),
            correct: Color::Rgb(184, 187, 38),
            incorrect: Color::Rgb(251, 73, 52),
            subtle: Color::Rgb(189, 174, 147),
            accent: Color::Rgb(131, 165, 152),
            border: Color::Rgb(80, 73, 69),
        }
    }

    pub fn tokyo_night() -> Theme {
        Theme {
            name: "tokyo_night".to_string(),
            bg: Color::Rgb(26, 27, 38),
            fg: Color::Rgb(192, 202, 245),
            cursor: Color::Rgb(192, 202, 245),
            correct: Color::Rgb(158, 206, 106),
            incorrect: Color::Rgb(247, 118, 142),
            subtle: Color::Rgb(169, 177, 214),
            accent: Color::Rgb(125, 167, 221),
            border: Color::Rgb(59, 66, 97),
        }
    }

    pub fn nord() -> Theme {
        Theme {
            name: "nord".to_string(),
            bg: Color::Rgb(46, 52, 64),
            fg: Color::Rgb(236, 239, 244),
            cursor: Color::Rgb(236, 239, 244),
            correct: Color::Rgb(163, 190, 140),
            incorrect: Color::Rgb(255, 154, 162),
            subtle: Color::Rgb(180, 187, 201),
            accent: Color::Rgb(136, 192, 208),
            border: Color::Rgb(59, 66, 82),
        }
    }

    pub fn monokai() -> Theme {
        Theme {
            name: "monokai".to_string(),
            bg: Color::Rgb(39, 40, 34),
            fg: Color::Rgb(248, 248, 242),
            cursor: Color::Rgb(255, 127, 80),
            correct: Color::Rgb(166, 226, 46),
            incorrect: Color::Rgb(255, 102, 153),
            subtle: Color::Rgb(190, 190, 175),
            accent: Color::Rgb(102, 217, 239),
            border: Color::Rgb(72, 72, 50),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_and_incorrect_text_meet_wcag_aa_contrast() {
        for theme in Theme::all() {
            for (role, color) in [
                ("foreground", theme.fg),
                ("cursor", theme.cursor),
                ("correct", theme.correct),
                ("incorrect", theme.incorrect),
                ("subtle", theme.subtle),
                ("accent", theme.accent),
            ] {
                assert!(
                    contrast(theme.bg, color) >= 4.5,
                    "{} {role} text contrast is too low",
                    theme.name
                );
            }
        }
    }

    fn contrast(first: Color, second: Color) -> f64 {
        let lighter = luminance(first).max(luminance(second));
        let darker = luminance(first).min(luminance(second));
        (lighter + 0.05) / (darker + 0.05)
    }

    fn luminance(color: Color) -> f64 {
        let Color::Rgb(red, green, blue) = color else {
            panic!("built-in themes must use RGB colors");
        };
        [red, green, blue]
            .into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(channel, weight)| {
                let channel = f64::from(channel) / 255.0;
                let linear = if channel <= 0.04045 {
                    channel / 12.92
                } else {
                    ((channel + 0.055) / 1.055).powf(2.4)
                };
                linear * weight
            })
            .sum()
    }
}
