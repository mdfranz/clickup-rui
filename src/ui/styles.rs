use ratatui::style::{Color, Modifier, Style};
use ratatui::Frame;

pub const COLOR_PRIMARY: Color = Color::Rgb(123, 97, 255); // ClickUp Purple
pub const COLOR_BG: Color = Color::Rgb(20, 20, 25);        // Dark Theme BG
pub const COLOR_FG: Color = Color::Rgb(240, 240, 245);     // Off-white FG
pub const COLOR_SUCCESS: Color = Color::Rgb(0, 200, 115);  // Vibrant Green
pub const COLOR_WARN: Color = Color::Rgb(255, 170, 0);     // Vibrant Orange
pub const COLOR_ERROR: Color = Color::Rgb(255, 75, 75);    // Vibrant Red
pub const COLOR_MUTED: Color = Color::Rgb(120, 120, 135);  // Gray

pub fn render_background(f: &mut Frame) {
    f.render_widget(
        ratatui::widgets::Block::default().style(Style::default().bg(COLOR_BG)),
        f.area(),
    );
}

pub fn style_title() -> Style {
    Style::default().fg(COLOR_PRIMARY).add_modifier(Modifier::BOLD).bg(COLOR_BG)
}

pub fn style_selected() -> Style {
    Style::default()
        .bg(COLOR_PRIMARY)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD)
}

pub fn style_border_active() -> Style {
    Style::default().fg(COLOR_PRIMARY)
}

pub fn style_border_inactive() -> Style {
    Style::default().fg(COLOR_MUTED)
}

pub fn get_status_color(status_name: &str) -> Color {
    match status_name.to_lowercase().as_str() {
        "in progress" | "active" => COLOR_PRIMARY,
        "in review" | "review" => COLOR_WARN,
        "blocked" => COLOR_ERROR,
        "scoping" | "planning" => Color::Cyan,
        "completed" | "closed" | "done" => COLOR_SUCCESS,
        _ => COLOR_MUTED,
    }
}

