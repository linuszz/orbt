use orbt_protocol::{AgentStatus, PaneId, PaneLayout};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear},
    Frame,
};

use crate::app::App;
use crate::tui::theme::*;

pub struct Card {
    pub pane: PaneId,
    pub slot: usize,
    pub focused: bool,
    pub rect: Rect,
}

/// Cards keep the columns' arrangement rather than being packed into a grid, so
/// the third card is the pane three rights away: a column holding three panes
/// stays a column. Every column takes the same width regardless of how many
/// panes it stacks, so a tall column does not squeeze its neighbours.
pub fn layout(
    node: &PaneLayout,
    area: Rect,
    focus: PaneId,
    panes: &std::collections::HashMap<PaneId, crate::app::PaneState>,
) -> Vec<Card> {
    let PaneLayout::Strip { columns, .. } = node else {
        return Vec::new();
    };

    let count = columns.iter().map(|c| c.panes.len()).sum::<usize>();
    if count == 0 {
        return Vec::new();
    }

    // Real pane sizes from the terminal grids, so a card keeps the aspect its
    // pane has on screen instead of stretching to fill the panel.
    let real_w = |pane: &PaneId| {
        panes
            .get(pane)
            .map(|p| p.parser.grid.cols.max(1))
            .unwrap_or(80) as usize
    };
    let real_h = |pane: &PaneId| {
        panes
            .get(pane)
            .map(|p| p.parser.grid.rows.max(1))
            .unwrap_or(24) as usize
    };
    let col_ws: Vec<usize> = columns
        .iter()
        .map(|c| c.panes.first().map(real_w).unwrap_or(80))
        .collect();
    let total_w: usize = col_ws.iter().sum();
    let tallest_h: usize = columns
        .iter()
        .map(|c| c.panes.iter().map(real_h).sum::<usize>())
        .max()
        .unwrap_or(1)
        .max(1);

    // Largest arrangement of the strip's aspect that fits the panel, centred.
    let scale = (area.width as f32 / total_w.max(1) as f32)
        .min(area.height as f32 / tallest_h as f32)
        .min(1.0);
    let arr_w = ((total_w as f32 * scale).round() as u16)
        .max(1)
        .min(area.width);
    let arr_h = ((tallest_h as f32 * scale).round() as u16)
        .max(1)
        .min(area.height);
    let origin = Rect {
        x: area.x + area.width.saturating_sub(arr_w) / 2,
        y: area.y + area.height.saturating_sub(arr_h) / 2,
        width: arr_w,
        height: arr_h,
    };

    let gap = 1u16;
    let usable = origin
        .width
        .saturating_sub(gap * (columns.len() as u16 - 1).min(origin.width));
    let mut cards = Vec::with_capacity(count);
    let mut slot = 0usize;
    let mut x = origin.x;
    for (ci, column) in columns.iter().enumerate() {
        let col_w = if ci + 1 == columns.len() {
            origin.x + origin.width - x
        } else {
            ((usable as usize * col_ws[ci]) / total_w) as u16
        };
        let col_w = col_w.max(3);
        let col_h_sum: usize = column.panes.iter().map(real_h).sum::<usize>().max(1);
        let rows_in_column = column.panes.len().max(1);
        let mut y = origin.y;
        for (ri, &pane) in column.panes.iter().enumerate() {
            let h = if ri + 1 == rows_in_column {
                origin.y + origin.height - y
            } else {
                ((origin.height as usize * real_h(&pane)) / col_h_sum) as u16
            };
            cards.push(Card {
                pane,
                slot,
                focused: pane == focus,
                rect: Rect {
                    x,
                    y,
                    width: col_w.min(origin.x + origin.width - x),
                    height: h.max(1),
                },
            });
            y += h;
            slot += 1;
        }
        x += col_w + gap;
    }
    cards
}

/// How far the open animation has run: 0 while the cards are points, 1 when
/// they fill their slots. Settled (or never animated) reads as 1.
fn open_progress(app: &App) -> f32 {
    app.overview_anim_start
        .and_then(|start| {
            crate::app::Anim {
                start_tick: start,
                duration: crate::app::OVERVIEW_ANIM_TICKS,
                from: 0.0,
                to: 1.0,
            }
            .value_at(app.tick_count)
        })
        .unwrap_or(1.0)
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let cards = layout(app.pane_tree(), area, app.active_pane, &app.panes);
    if cards.is_empty() {
        return;
    }

    let total = cards.len();
    let block = Block::default()
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().bg(bg_secondary()).fg(fg_primary()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border()))
        .title(Span::styled(
            format!(" Strip Overview — {total} panes "),
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(Clear, area);
    frame.render_widget(block, area);

    let inner = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    };

    let leaves = app.pane_tree().leaves();
    let progress = open_progress(app);
    for card in &cards {
        render_card(frame, inner, card, app, &leaves, total, progress);
    }
}

fn render_card(
    frame: &mut Frame,
    area: Rect,
    card: &Card,
    app: &App,
    leaves: &[PaneId],
    total: usize,
    progress: f32,
) {
    let rect = crate::tui::grow_rect(card.rect, progress).intersection(area);
    if rect.width < 3 || rect.height < 3 {
        return;
    }

    let edge = if card.focused { accent() } else { border_dim() };
    let block = Block::default()
        .border_type(ratatui::widgets::BorderType::Rounded)
        .style(Style::default().bg(bg_secondary()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(edge).add_modifier(if card.focused {
            Modifier::BOLD
        } else {
            Modifier::empty()
        }));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let idx = leaves.iter().position(|&p| p == card.pane).unwrap_or(0) + 1;
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(" {idx}/{total}"),
            Style::default().fg(if card.focused {
                accent_idle()
            } else {
                fg_muted()
            }),
        ),
        Span::styled(
            if card.focused { " *" } else { "  " },
            Style::default()
                .fg(accent_idle())
                .add_modifier(Modifier::BOLD),
        ),
    ])];

    // Terminal text cannot be scaled down to a thumbnail, so a card says what
    // the pane is for instead of pretending to show it.
    let where_ = app
        .panes
        .get(&card.pane)
        .map(|p| p.cwd.as_str())
        .unwrap_or_default();
    lines.push(Line::from(Span::styled(
        fit_cwd(where_, inner.width),
        Style::default().fg(fg_secondary()),
    )));

    if let Some(state) = agent_line(app, card.pane) {
        lines.push(Line::from(state));
    }

    for (row, line) in lines.iter().enumerate() {
        let y = inner.y + row as u16;
        if y >= inner.y + inner.height {
            break;
        }
        frame.render_widget(
            line.clone(),
            Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: 1,
            },
        );
    }
}

/// The agent in this pane, which is how you find the one an agent is in.
fn agent_line(app: &App, pane: PaneId) -> Option<Vec<Span<'static>>> {
    let agent = app.agents.iter().find(|a| a.pane_id == Some(pane))?;
    let (icon, label, colour) = match agent.status {
        AgentStatus::Working => ("◉", "working", accent_idle()),
        AgentStatus::Blocked => ("◍", "blocked", accent_blocked()),
        AgentStatus::Idle => ("○", "idle", fg_muted()),
        AgentStatus::Done => ("●", "done", fg_muted()),
        AgentStatus::Error => ("◌", "error", accent_error()),
    };
    Some(vec![
        Span::styled(format!("{icon} "), Style::default().fg(colour)),
        Span::styled(
            label.to_string(),
            Style::default().fg(colour).add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Show the end of a path, which is the part that identifies it, truncated to
/// whatever the card can hold.
pub fn fit_cwd(cwd: &str, width: u16) -> String {
    if cwd.is_empty() {
        return "—".to_string();
    }
    let room = width.max(2) as usize;
    let trimmed = cwd.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/".to_string();
    }
    if trimmed.chars().count() <= room {
        return trimmed.to_string();
    }
    let chars: Vec<char> = trimmed.chars().collect();
    let keep = room - 1;
    let start = chars.len().saturating_sub(keep);
    let mut tail: String = chars[start..].iter().collect();
    // Cut at a separator where one is close by, so the tail still reads as a
    // path rather than as the tail of a word.
    if start > 0 && !tail.starts_with('/') {
        if let Some(cut) = tail.find('/') {
            tail = tail[cut..].to_string();
        }
    }
    format!("…{tail}")
}

pub fn card_at(cards: &[Card], col: u16, row: u16) -> Option<PaneId> {
    cards
        .iter()
        .find(|c| {
            col >= c.rect.x
                && col < c.rect.x + c.rect.width
                && row >= c.rect.y
                && row < c.rect.y + c.rect.height
        })
        .map(|c| c.pane)
}

#[cfg(test)]
mod tests {
    use super::*;
    use orbt_protocol::StripColumn;

    #[test]
    fn grow_scales_around_the_centre_and_settles() {
        let rect = Rect {
            x: 10,
            y: 10,
            width: 40,
            height: 20,
        };
        let half = crate::tui::grow_rect(rect, 0.5);
        assert_eq!(half.width, 20);
        assert_eq!(half.height, 10);
        // Same centre: 10+20 == 10+20, 10+10 == 10+10.
        assert_eq!(half.x + half.width / 2, rect.x + rect.width / 2);
        assert_eq!(half.y + half.height / 2, rect.y + rect.height / 2);

        let start = crate::tui::grow_rect(rect, 0.0);
        assert_eq!(start.width, 0, "a closed overview is a point");
        assert_eq!(
            crate::tui::grow_rect(rect, 1.0),
            rect,
            "settled means untouched"
        );
    }

    #[test]
    fn every_column_gets_the_same_width_regardless_of_stack_height() {
        // Two columns: one stacks three panes, the other one. They must take
        // equal width, with the tall column dividing its height instead.
        let tree = PaneLayout::Strip {
            columns: vec![
                StripColumn {
                    panes: vec![PaneId(1), PaneId(2), PaneId(3)],
                    width: 0.0,
                },
                StripColumn {
                    panes: vec![PaneId(4)],
                    width: 0.0,
                },
            ],
            column_width: 80,
        };
        let area = Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 30,
        };
        let cards = layout(&tree, area, PaneId(1), &std::collections::HashMap::new());
        let col1_w = cards
            .iter()
            .find(|c| c.pane == PaneId(1))
            .unwrap()
            .rect
            .width;
        let col2_w = cards
            .iter()
            .find(|c| c.pane == PaneId(4))
            .unwrap()
            .rect
            .width;
        assert!(
            col1_w.abs_diff(col2_w) <= 1,
            "stacked ({col1_w}) and single ({col2_w}) columns match in width"
        );
        let tall_h = cards
            .iter()
            .find(|c| c.pane == PaneId(1))
            .unwrap()
            .rect
            .height;
        let flat_h = cards
            .iter()
            .find(|c| c.pane == PaneId(4))
            .unwrap()
            .rect
            .height;
        assert!(
            flat_h > tall_h * 2,
            "the single pane gets the column's height"
        );
    }

    #[test]
    fn cards_keep_the_panes_real_aspect() {
        // One column of two panes whose terminal heights differ 1:3 — the
        // cards must divide the column in the same 1:3, not evenly.
        let mut panes = std::collections::HashMap::new();
        let mut short = crate::app::PaneState::new(80, 6);
        short.parser.grid.resize(80, 6);
        panes.insert(PaneId(1), short);
        let mut tall = crate::app::PaneState::new(80, 18);
        tall.parser.grid.resize(80, 18);
        panes.insert(PaneId(2), tall);
        let tree = PaneLayout::Strip {
            columns: vec![StripColumn {
                panes: vec![PaneId(1), PaneId(2)],
                width: 0.0,
            }],
            column_width: 0,
        };
        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 24,
        };
        let cards = layout(&tree, area, PaneId(1), &panes);
        let h1 = cards
            .iter()
            .find(|c| c.pane == PaneId(1))
            .unwrap()
            .rect
            .height;
        let h2 = cards
            .iter()
            .find(|c| c.pane == PaneId(2))
            .unwrap()
            .rect
            .height;
        assert!(
            (h2 as f32 / h1.max(1) as f32 - 3.0).abs() < 0.6,
            "heights follow the real 1:3 split, got {h1}:{h2}"
        );

        // Two equal panes side by side keep roughly square-ish cells: the
        // arrangement is limited by the panel's narrower dimension and centred.
        let mut panes2 = std::collections::HashMap::new();
        panes2.insert(PaneId(1), crate::app::PaneState::new(80, 24));
        panes2.insert(PaneId(2), crate::app::PaneState::new(80, 24));
        let tree2 = PaneLayout::Strip {
            columns: vec![
                StripColumn {
                    panes: vec![PaneId(1)],
                    width: 0.0,
                },
                StripColumn {
                    panes: vec![PaneId(2)],
                    width: 0.0,
                },
            ],
            column_width: 0,
        };
        let wide_area = Rect {
            x: 0,
            y: 0,
            width: 200,
            height: 20,
        };
        let cards2 = layout(&tree2, wide_area, PaneId(1), &panes2);
        let total_w: u16 = cards2.iter().map(|c| c.rect.width).sum();
        assert!(
            total_w < wide_area.width,
            "aspect preserved by letterboxing, not stretching: {total_w} < 200"
        );
    }

    #[test]
    fn open_progress_is_full_when_never_animated() {
        let app = crate::app::tests::make_test_app(120, 30);
        assert_eq!(open_progress(&app), 1.0);
    }

    fn strip() -> PaneLayout {
        PaneLayout::Strip {
            columns: vec![
                StripColumn {
                    panes: vec![PaneId(1), PaneId(2)],
                    width: 0.0,
                },
                StripColumn::single(PaneId(3)),
            ],
            column_width: 40,
        }
    }

    #[test]
    fn cards_keep_the_strip_order_and_the_focus() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 20,
        };
        let cards = layout(&strip(), area, PaneId(3), &std::collections::HashMap::new());
        assert_eq!(
            cards.iter().map(|c| c.pane).collect::<Vec<_>>(),
            vec![PaneId(1), PaneId(2), PaneId(3)],
            "left to right, top to bottom, matching the strip"
        );
        assert_eq!(cards[2].slot, 2);
        assert!(cards[2].focused, "the focused pane is marked");
        assert!(!cards[0].focused);
    }

    #[test]
    fn a_column_keeps_its_panes_stacked_in_one_place() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 20,
        };
        let cards = layout(&strip(), area, PaneId(1), &std::collections::HashMap::new());
        assert_eq!(cards[0].rect.x, cards[1].rect.x, "same column, same x");
        assert!(cards[0].rect.y < cards[1].rect.y, "and one above the other");
        assert!(
            cards[2].rect.x > cards[1].rect.x,
            "the next column sits to the right"
        );
    }

    #[test]
    fn cards_tile_the_area_without_overlapping() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 20,
        };
        let cards = layout(&strip(), area, PaneId(1), &std::collections::HashMap::new());
        for (i, a) in cards.iter().enumerate() {
            for b in &cards[i + 1..] {
                let apart = a.rect.x + a.rect.width <= b.rect.x
                    || b.rect.x + b.rect.width <= a.rect.x
                    || a.rect.y + a.rect.height <= b.rect.y
                    || b.rect.y + b.rect.height <= a.rect.y;
                assert!(apart, "{:?} overlaps {:?}", a.rect, b.rect);
            }
        }
    }

    #[test]
    fn a_click_resolves_to_the_card_under_the_cursor() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 20,
        };
        let cards = layout(&strip(), area, PaneId(1), &std::collections::HashMap::new());
        let third = cards[2].rect;
        assert_eq!(
            card_at(&cards, third.x, third.y + 1),
            Some(PaneId(3)),
            "clicking a card focuses that pane"
        );
        assert_eq!(card_at(&cards, third.x - 1, third.y), None);
    }

    #[test]
    fn a_long_path_is_shown_from_its_end() {
        assert_eq!(
            fit_cwd("/a/very/deeply/nested/project/directory", 12),
            "…/directory",
            "the tail keeps its separator so it still reads as a path"
        );
        assert_eq!(fit_cwd("/home/linus", 40), "/home/linus");
        assert_eq!(fit_cwd("/", 10), "/", "the root is still a path");
        assert_eq!(fit_cwd("", 10), "—", "an unknown cwd is not a blank row");
    }

    #[test]
    fn nothing_to_show_outside_a_strip() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 20,
        };
        assert!(layout(
            &PaneLayout::Leaf(PaneId(1)),
            area,
            PaneId(1),
            &std::collections::HashMap::new()
        )
        .is_empty());
    }
}
