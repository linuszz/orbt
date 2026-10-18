use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, PluginSurfaceFrame};
use crate::tui::term_color;
use crate::tui::theme::*;

const HINT: &str = "PLUGIN DOCK · Enter send · Esc exit";

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    if area.width < 4 || area.height < 4 {
        return;
    }
    let border_color = if app.plugin_dock_focused {
        accent()
    } else {
        border()
    };
    let block = Block::default().borders(Borders::ALL).border_style(
        Style::default().fg(border_color),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input_h = 1u16;
    let hint_h = u16::from(app.plugin_dock_focused);
    let surfaces_h = inner.height.saturating_sub(input_h + hint_h);
    if surfaces_h == 0 {
        return;
    }

    let mut y = inner.y;
    for surface in app.plugin_surfaces.values() {
        let remaining = inner.y + surfaces_h - y;
        if remaining == 0 {
            break;
        }
        let h = surface.height.min(remaining);
        draw_surface(
            frame,
            surface,
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: h,
            },
        );
        y += h;
    }

    let input_area = Rect {
        x: inner.x,
        y: inner.y + surfaces_h,
        width: inner.width,
        height: input_h,
    };
    let cursor = if app.plugin_dock_focused {
        "\u{2588}"
    } else {
        ""
    };
    let style = if app.plugin_dock_focused {
        Style::default().fg(fg_primary())
    } else {
        Style::default().fg(fg_muted())
    };
    let line = format!("> {}{}", app.plugin_dock_input, cursor);
    frame.render_widget(Paragraph::new(line).style(style), input_area);

    if app.plugin_dock_focused && hint_h > 0 {
        let hint_area = Rect {
            x: inner.x,
            y: inner.y + surfaces_h + input_h,
            width: inner.width,
            height: hint_h,
        };
        frame.render_widget(
            Paragraph::new(HINT).style(Style::default().fg(fg_muted())),
            hint_area,
        );
    }
}

fn draw_surface(frame: &mut Frame, surface: &PluginSurfaceFrame, area: Rect) {
    let buf = frame.buffer_mut();
    let w = surface.width.max(1);
    let h = surface.height.min(area.height);
    for dy in 0..h {
        for dx in 0..area.width.min(w) {
            let idx = (dy * w + dx) as usize;
            if let Some(cell) = surface.cells.get(idx) {
                let rc = buf.get_mut(area.x + dx, area.y + dy);
                rc.set_char(cell.ch);
                rc.set_fg(term_color(&cell.fg));
                rc.set_bg(term_color(&cell.bg));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbt_protocol::Cell;

    #[test]
    fn renders_surface_cells_and_input_line() {
        let mut app = crate::app::tests::make_test_app(80, 24);
        let cell = Cell {
            ch: 'A',
            ..Cell::default()
        };
        app.plugin_surfaces.insert(
            0,
            PluginSurfaceFrame {
                plugin_id: "orbit-ai/nested-agent".to_string(),
                width: 2,
                height: 1,
                cells: vec![cell; 2],
            },
        );
        app.plugin_dock_focused = true;
        app.plugin_dock_input = "hi".to_string();

        let backend = ratatui::backend::TestBackend::new(30, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render(f, Rect::new(0, 0, 30, 10), &app))
            .unwrap();

        let buf = terminal.backend().buffer();
        assert_eq!(buf[(1, 1)].symbol(), "A");
        let input_row = (0..30u16)
            .map(|x| buf[(x, 7)].symbol().to_string())
            .collect::<String>();
        assert!(input_row.contains("> hi"), "got: {}", input_row);
        let hint_row = (0..30u16)
            .map(|x| buf[(x, 8)].symbol().to_string())
            .collect::<String>();
        assert!(hint_row.contains("PLUGIN DOCK"), "got: {}", hint_row);
    }

    #[test]
    fn unfocused_hides_hint_and_cursor() {
        let mut app = crate::app::tests::make_test_app(80, 24);
        app.plugin_surfaces.insert(
            0,
            PluginSurfaceFrame {
                plugin_id: "p".to_string(),
                width: 1,
                height: 1,
                cells: vec![Cell {
                    ch: ' ',
                    ..Cell::default()
                }],
            },
        );
        app.plugin_dock_focused = false;
        app.plugin_dock_input = "x".to_string();

        let backend = ratatui::backend::TestBackend::new(30, 10);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render(f, Rect::new(0, 0, 30, 10), &app))
            .unwrap();

        let buf = terminal.backend().buffer();
        let last_row = (0..30u16)
            .map(|x| buf[(x, 9)].symbol().to_string())
            .collect::<String>();
        assert!(!last_row.contains("PLUGIN DOCK"), "got: {}", last_row);
    }

    #[test]
    fn tiny_area_renders_nothing_without_panic() {
        let app = crate::app::tests::make_test_app(80, 24);
        let backend = ratatui::backend::TestBackend::new(3, 2);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render(f, Rect::new(0, 0, 3, 2), &app))
            .unwrap();
    }
}
