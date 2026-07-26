use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear},
    Frame,
};

use crate::app::{App, ContextMenuItem, InputMode};
use crate::tui::theme::*;

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(menu) = &app.context_menu {
        let max_label_len = menu
            .items
            .iter()
            .filter_map(|i| match i {
                ContextMenuItem::Action {
                    label, shortcut, ..
                } => Some(label.len() + shortcut.len() + 2),
                _ => None,
            })
            .max()
            .unwrap_or(16);
        let w = (max_label_len as u16 + 4).min(area.width.saturating_sub(2));
        let h = (menu.items.len() as u16 + 2).min(area.height.saturating_sub(2));
        let x = menu.x.min(area.width.saturating_sub(w));
        let y = menu.y.min(area.height.saturating_sub(h));
        let menu_area = Rect {
            x,
            y,
            width: w,
            height: h,
        };

        frame.render_widget(Clear, menu_area);

        let block = Block::default()
            .style(Style::default().bg(bg_secondary()).fg(fg_primary()))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border()));
        frame.render_widget(block, menu_area);

        for (i, item) in menu.items.iter().enumerate() {
            let row_y = menu_area.y + 1 + i as u16;
            // Stop rendering if we've reached the bottom border
            if row_y + 1 >= menu_area.y + menu_area.height {
                break;
            }
            match item {
                ContextMenuItem::Action {
                    label, shortcut, ..
                } => {
                    let is_selected = i == menu.selected;
                    let bg = if is_selected {
                        bg_primary()
                    } else {
                        bg_secondary()
                    };
                    let label_style = if is_selected {
                        Style::default()
                            .fg(fg_primary())
                            .bg(bg)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(fg_secondary()).bg(bg)
                    };
                    let shortcut_style = Style::default().fg(accent()).bg(bg);

                    // inner_w excludes the two border columns
                    let inner_w = menu_area.width.saturating_sub(2) as usize;
                    let label_span = Span::styled(format!(" {label}"), label_style);
                    let mut spans = vec![label_span];
                    if !shortcut.is_empty() {
                        let pad = inner_w.saturating_sub(label.len() + shortcut.len() + 3);
                        spans.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
                        spans.push(Span::styled(shortcut.clone(), shortcut_style));
                    }

                    let line = Line::from(spans).style(Style::default().bg(bg));
                    frame.render_widget(
                        line,
                        Rect {
                            x: menu_area.x + 1,
                            y: row_y,
                            width: menu_area.width.saturating_sub(2),
                            height: 1,
                        },
                    );
                }
                ContextMenuItem::Separator => {
                    let inner_w = menu_area.width.saturating_sub(2) as usize;
                    let line = Line::from(Span::styled(
                        "\u{2500}".repeat(inner_w),
                        Style::default().fg(border()),
                    ));
                    frame.render_widget(
                        line,
                        Rect {
                            x: menu_area.x + 1,
                            y: row_y,
                            width: menu_area.width.saturating_sub(2),
                            height: 1,
                        },
                    );
                }
            }
        }
    }

    if matches!(app.mode, InputMode::CommandPalette { .. }) {
        super::command_palette::render(frame, area, app);
    }
}
