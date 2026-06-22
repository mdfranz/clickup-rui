pub mod spinner;
pub mod styles;
pub mod terminal;

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};
use ratatui::Frame;

pub fn get_popup_layout(r: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage((100 - percent_y) / 2),
                Constraint::Percentage(percent_y),
                Constraint::Percentage((100 - percent_y) / 2),
            ]
            .as_ref(),
        )
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            [
                Constraint::Percentage((100 - percent_x) / 2),
                Constraint::Percentage(percent_x),
                Constraint::Percentage((100 - percent_x) / 2),
            ]
            .as_ref(),
        )
        .split(popup_layout[1])[1]
}

pub fn render_comment_editor(f: &mut Frame, size: Rect, comment_buffer: &str) {
    let popup_layout = get_popup_layout(size, 50, 30);
    f.render_widget(Clear, popup_layout);

    let inner_width = (popup_layout.width as usize).saturating_sub(6);
    let wrapped_lines = crate::util::format::wrap_text_by_chars(comment_buffer, inner_width);

    let paragraph_lines: Vec<Line> = wrapped_lines
        .iter()
        .map(|l| Line::from(l.as_str()))
        .collect();

    let editor_p = Paragraph::new(paragraph_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Add Comment (Ctrl+s to post, Esc to close) ")
                .border_style(styles::style_border_active())
                .padding(Padding::new(2, 2, 1, 1)),
        )
        .style(
            Style::default()
                .add_modifier(Modifier::empty())
                .fg(styles::COLOR_FG)
                .bg(styles::COLOR_BG),
        );
    f.render_widget(editor_p, popup_layout);

    let cursor_row = wrapped_lines.len().saturating_sub(1) as u16;
    let cursor_col = wrapped_lines.last().map(|l| l.chars().count()).unwrap_or(0) as u16;
    let cursor_y = popup_layout.y + 2 + cursor_row;
    let cursor_x = popup_layout.x + 3 + cursor_col;
    let safe_cursor_x = cursor_x.min(popup_layout.x + popup_layout.width.saturating_sub(2));
    let safe_cursor_y = cursor_y.min(popup_layout.y + popup_layout.height.saturating_sub(2));
    f.set_cursor_position(ratatui::layout::Position::new(safe_cursor_x, safe_cursor_y));
}
