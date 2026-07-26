use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

use crate::app::{App, MobileCloseTarget};
use crate::tui::theme::*;

/// Hit-test result for a mouse click on the confirm modal.
#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmHit {
    Cancel,
    Confirm,
    Outside,
}

/// Compute the modal's outer rect, centered within `area`.
pub fn modal_rect(area: Rect) -> Rect {
    let w = 30u16.min(area.width.saturating_sub(2));
    let h = 5u16;
    let x = area.x + area.width.saturating_sub(w) / 2;
    let y = area.y + area.height.saturating_sub(h) / 2;
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

fn inner_rect(modal: Rect) -> Rect {
    Rect {
        x: modal.x + 1,
        y: modal.y + 1,
        width: modal.width.saturating_sub(2),
        height: modal.height.saturating_sub(2),
    }
}

/// Hit-test a mouse click against the confirm modal.
/// Returns `Outside` if the click is not on a button.
pub fn hit_test(col: u16, row: u16, area: Rect) -> ConfirmHit {
    let modal = modal_rect(area);
    if col < modal.x
        || col >= modal.x + modal.width
        || row < modal.y
        || row >= modal.y + modal.height
    {
        return ConfirmHit::Outside;
    }
    let inner = inner_rect(modal);
    // Buttons are on the last inner row (row index 2 of 3).
    let btn_row = inner.y + 2;
    if row != btn_row {
        return ConfirmHit::Outside;
    }
    let mid_x = inner.x + inner.width / 2;
    if col < mid_x {
        ConfirmHit::Cancel
    } else {
        ConfirmHit::Confirm
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let confirm = match &app.mobile_close_confirm {
        Some(c) => c,
        None => return,
    };

    let modal = modal_rect(area);
    let inner = inner_rect(modal);

    frame.render_widget(Clear, modal);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border()))
            .style(Style::default().bg(bg_secondary())),
        modal,
    );

    // Row 0: title
    let (kind, name) = match &confirm.target {
        MobileCloseTarget::Space(idx) => (
            "Space",
            app.spaces.get(*idx).map(|s| s.name.as_str()).unwrap_or("?"),
        ),
        MobileCloseTarget::Tab(idx) => (
            "Tab",
            app.tabs.get(*idx).map(|t| t.name.as_str()).unwrap_or("?"),
        ),
    };
    let title = format!("Close {} \"{}\"?", kind, name);
    let title_str: String = title.chars().take(inner.width as usize).collect();
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            title_str,
            Style::default()
                .fg(fg_primary())
                .bg(bg_secondary())
                .add_modifier(Modifier::BOLD),
        ))),
        Rect {
            x: inner.x,
            y: inner.y,
            width: inner.width,
            height: 1,
        },
    );

    // Row 2: buttons — left half = Cancel, right half = Confirm
    let half_w = inner.width / 2;
    let right_w = inner.width - half_w;

    let cancel_style = if !confirm.confirm_focused {
        Style::default()
            .fg(bg_primary())
            .bg(accent())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg_muted()).bg(bg_secondary())
    };
    let confirm_style = if confirm.confirm_focused {
        Style::default()
            .fg(bg_primary())
            .bg(accent_error())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(fg_muted()).bg(bg_secondary())
    };

    // Center "[Cancel]" in left half, "[Confirm]" in right half
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{:^fill$}", "[Cancel]", fill = half_w as usize),
            cancel_style,
        ))),
        Rect {
            x: inner.x,
            y: inner.y + 2,
            width: half_w,
            height: 1,
        },
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{:^fill$}", "[Confirm]", fill = right_w as usize),
            confirm_style,
        ))),
        Rect {
            x: inner.x + half_w,
            y: inner.y + 2,
            width: right_w,
            height: 1,
        },
    );
}
