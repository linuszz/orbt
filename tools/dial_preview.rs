use std::env;
use std::io::Write;
use std::time::Instant;

use ratatui::style::Color;

use orbt_tui::theme::{
    accent, accent_blocked, accent_error, accent_idle, bg_card, bg_primary, border_dim, fg_muted,
    fg_primary, fg_secondary, set_theme, ALL_THEMES,
};

const TICK_MS: u64 = 16;
const WORKING_PERIOD: u64 = 48;
const BLOCKED_PERIOD: u64 = 48;
const ERROR_PERIOD: u64 = 60;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    N,
    E,
    S,
    W,
}

impl Slot {
    fn from_working_tick(tick: u64) -> Self {
        match tick / 12 {
            0 => Slot::N,
            1 => Slot::E,
            2 => Slot::S,
            3 => Slot::W,
            _ => Slot::N,
        }
    }

    fn trail_for_working_tick(tick: u64) -> Option<Slot> {
        match tick / 12 {
            0 => None,
            1 => Some(Slot::N),
            2 => Some(Slot::E),
            3 => Some(Slot::S),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SatelliteGlyph {
    Solid,
    Hollow,
    Circle,
    Trail,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DialState {
    Working,
    Blocked,
    Idle,
    Error,
}

fn lerp_rgb(c1: Color, c2: Color, t: f64) -> Color {
    let (r1, g1, b1) = color_to_rgb(c1);
    let (r2, g2, b2) = color_to_rgb(c2);
    let r = (r1 as f64 + (r2 as f64 - r1 as f64) * t) as u8;
    let g = (g1 as f64 + (g2 as f64 - g1 as f64) * t) as u8;
    let b = (b1 as f64 + (b2 as f64 - b1 as f64) * t) as u8;
    Color::Rgb(r, g, b)
}

fn color_to_rgb(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(i) => indexed_to_rgb(i),
        Color::Reset => (0, 0, 0),
        _ => (0, 0, 0),
    }
}

fn indexed_to_rgb(i: u8) -> (u8, u8, u8) {
    if i < 8 {
        match i {
            0 => (0, 0, 0),
            1 => (128, 0, 0),
            2 => (0, 128, 0),
            3 => (128, 128, 0),
            4 => (0, 0, 128),
            5 => (128, 0, 128),
            6 => (0, 128, 128),
            7 => (192, 192, 192),
            _ => (128, 128, 128),
        }
    } else if i < 16 {
        match i {
            8 => (128, 128, 128),
            9 => (255, 0, 0),
            10 => (0, 255, 0),
            11 => (255, 255, 0),
            12 => (0, 0, 255),
            13 => (255, 0, 255),
            14 => (0, 255, 255),
            15 => (255, 255, 255),
            _ => (128, 128, 128),
        }
    } else {
        let idx = (i - 16) as usize;
        let r = ((idx / 36) % 6) as u8 * 51;
        let g = ((idx / 6) % 6) as u8 * 51;
        let b = (idx % 6) as u8 * 51;
        (r, g, b)
    }
}

fn vwidth(s: &str) -> usize {
    let mut vis = 0usize;
    let mut esc = false;
    let mut csi = false;
    for ch in s.chars() {
        if esc {
            esc = false;
            if ch == '[' {
                csi = true;
            }
            continue;
        }
        if csi {
            if ('\u{40}'..='\u{7e}').contains(&ch) {
                csi = false;
            }
            continue;
        }
        if ch == '\u{1b}' {
            esc = true;
            continue;
        }
        vis += 1;
    }
    vis
}

fn pad_end(s: &str, width: usize) -> String {
    let w = vwidth(s);
    if w >= width {
        return s.to_string();
    }
    format!("{}{}", s, " ".repeat(width - w))
}

fn pad_start(s: &str, width: usize) -> String {
    let pad = width.saturating_sub(vwidth(s));
    format!("{}{}", " ".repeat(pad), s)
}

fn ansi_fg(c: Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("\x1b[38;2;{};{};{}m", r, g, b),
        _ => {
            let (r, g, b) = color_to_rgb(c);
            format!("\x1b[38;2;{};{};{}m", r, g, b)
        }
    }
}

fn ansi_reset() -> String {
    "\x1b[0m".to_string()
}

fn ansi_bold() -> String {
    "\x1b[1m".to_string()
}

fn ansi_dim() -> String {
    "\x1b[2m".to_string()
}

struct DialFrame {
    rows: [[&'static str; 3]; 3],
    row_colors: [[Color; 3]; 3],
}

impl DialFrame {
    fn base(ring_color: Color) -> Self {
        DialFrame {
            rows: [
                ["\u{256d}", "\u{2500}", "\u{256e}"],
                ["\u{2502}", " ", "\u{2502}"],
                ["\u{2570}", "\u{2500}", "\u{256f}"],
            ],
            row_colors: [[ring_color; 3]; 3],
        }
    }

    fn render_dial(state: DialState, tick: u64, no_animation: bool) -> (Self, Option<Slot>) {
        match state {
            DialState::Working => {
                if no_animation {
                    return DialFrame::degradation_working();
                }
                let slot = Slot::from_working_tick(tick % WORKING_PERIOD);
                let trail_slot = Slot::trail_for_working_tick(tick % WORKING_PERIOD);
                let sat_color = accent();
                let trail_color = lerp_rgb(accent(), bg_primary(), 0.4);

                let mut frame = DialFrame::base(border_dim());

                if let Some(ts) = trail_slot {
                    let (tc, _tc_color) =
                        Self::slot_content(ts, SatelliteGlyph::Trail, trail_color);
                    let trail_cell_color = if ts == Slot::S {
                        fg_muted()
                    } else {
                        trail_color
                    };
                    let (row, col) = Self::slot_row_col(ts);
                    frame.rows[row][col] = tc;
                    frame.row_colors[row][col] = trail_cell_color;
                }

                let in_shadow = slot == Slot::S;
                let sat_glyph = if in_shadow {
                    SatelliteGlyph::Hollow
                } else {
                    SatelliteGlyph::Solid
                };
                let sat_color_final = if in_shadow { fg_muted() } else { sat_color };
                let (sc, sc_color) = Self::slot_content(slot, sat_glyph, sat_color_final);
                let (row, col) = Self::slot_row_col(slot);
                frame.rows[row][col] = sc;
                frame.row_colors[row][col] = sc_color;

                (frame, Some(slot))
            }
            DialState::Blocked => {
                if no_animation {
                    return DialFrame::degradation_blocked();
                }
                let bright = tick % BLOCKED_PERIOD < BLOCKED_PERIOD / 2;
                let ring_color = if bright {
                    accent_blocked()
                } else {
                    border_dim()
                };
                let sat_color = if bright { accent_blocked() } else { fg_muted() };

                let mut frame = DialFrame::base(ring_color);

                let (sc, _) = Self::slot_content(Slot::S, SatelliteGlyph::Hollow, sat_color);
                let (row, col) = Self::slot_row_col(Slot::S);
                frame.rows[row][col] = sc;
                frame.row_colors[row][col] = sat_color;

                (frame, Some(Slot::S))
            }
            DialState::Idle => {
                let ring_color = border_dim();
                let sat_color = accent_idle();

                let mut frame = DialFrame::base(ring_color);

                let (sc, _) = Self::slot_content(Slot::W, SatelliteGlyph::Circle, sat_color);
                let (row, col) = Self::slot_row_col(Slot::W);
                frame.rows[row][col] = sc;
                frame.row_colors[row][col] = sat_color;

                (frame, Some(Slot::W))
            }
            DialState::Error => {
                if no_animation {
                    return DialFrame::degradation_error();
                }
                let slot = if (tick % ERROR_PERIOD) < (ERROR_PERIOD / 2) {
                    Slot::N
                } else {
                    Slot::S
                };
                let sat_color = accent_error();
                let ring_color = border_dim();

                let mut frame = DialFrame::base(ring_color);

                let (sc, _) = Self::slot_content(slot, SatelliteGlyph::Solid, sat_color);
                let (row, col) = Self::slot_row_col(slot);
                frame.rows[row][col] = sc;
                frame.row_colors[row][col] = sat_color;

                (frame, Some(slot))
            }
        }
    }

    fn degradation_working() -> (Self, Option<Slot>) {
        let sat_color = accent();
        let ring_color = border_dim();

        let mut frame = DialFrame::base(ring_color);
        let (sc, _) = Self::slot_content(Slot::E, SatelliteGlyph::Solid, sat_color);
        let (row, col) = Self::slot_row_col(Slot::E);
        frame.rows[row][col] = sc;
        frame.row_colors[row][col] = sat_color;

        (frame, Some(Slot::E))
    }

    fn degradation_blocked() -> (Self, Option<Slot>) {
        let sat_color = accent_blocked();
        let ring_color = accent_blocked();

        let mut frame = DialFrame::base(ring_color);
        let (sc, _) = Self::slot_content(Slot::S, SatelliteGlyph::Hollow, sat_color);
        let (row, col) = Self::slot_row_col(Slot::S);
        frame.rows[row][col] = sc;
        frame.row_colors[row][col] = sat_color;

        (frame, Some(Slot::S))
    }

    fn degradation_error() -> (Self, Option<Slot>) {
        let sat_color = accent_error();
        let ring_color = border_dim();

        let mut frame = DialFrame::base(ring_color);
        let (sc, _) = Self::slot_content(Slot::E, SatelliteGlyph::Solid, sat_color);
        let (row, col) = Self::slot_row_col(Slot::E);
        frame.rows[row][col] = sc;
        frame.row_colors[row][col] = sat_color;

        (frame, Some(Slot::E))
    }

    fn slot_content(
        slot: Slot,
        glyph: SatelliteGlyph,
        glyph_color: Color,
    ) -> (&'static str, Color) {
        match (slot, glyph) {
            (Slot::N, SatelliteGlyph::Solid) => ("\u{25cf}", glyph_color),
            (Slot::N, SatelliteGlyph::Hollow) => {
                panic!("N slot must be solid — glyph is sandwiched between ╭ and ╮")
            }
            (Slot::N, SatelliteGlyph::Circle) => {
                panic!("N slot must be solid — glyph is sandwiched between ╭ and ╮")
            }
            (Slot::E, SatelliteGlyph::Solid) => ("\u{25cf}", glyph_color),
            (Slot::E, SatelliteGlyph::Hollow) => ("\u{25cc}", glyph_color),
            (Slot::E, SatelliteGlyph::Circle) => ("\u{25cb}", glyph_color),
            (Slot::S, SatelliteGlyph::Solid) => ("\u{25cf}", glyph_color),
            (Slot::S, SatelliteGlyph::Hollow) => ("\u{25cc}", glyph_color),
            (Slot::S, SatelliteGlyph::Circle) => ("\u{25cb}", glyph_color),
            (Slot::W, SatelliteGlyph::Solid) => ("\u{25cf}", glyph_color),
            (Slot::W, SatelliteGlyph::Hollow) => ("\u{25cc}", glyph_color),
            (Slot::W, SatelliteGlyph::Circle) => ("\u{25cb}", glyph_color),
            (Slot::N, SatelliteGlyph::Trail) => ("\u{00b7}", glyph_color),
            (Slot::E, SatelliteGlyph::Trail) => ("\u{00b7}", glyph_color),
            (Slot::S, SatelliteGlyph::Trail) => ("\u{00b7}", glyph_color),
            (Slot::W, SatelliteGlyph::Trail) => ("\u{00b7}", glyph_color),
        }
    }

    fn slot_row_col(slot: Slot) -> (usize, usize) {
        match slot {
            Slot::N => (0, 1),
            Slot::E => (1, 2),
            Slot::S => (2, 1),
            Slot::W => (1, 0),
        }
    }
}

struct CliArgs {
    theme_name: String,
    all_themes: bool,
    no_animation: bool,
    slow: bool,
    tps: u64,
    full: bool,
}

impl CliArgs {
    fn parse() -> Self {
        let mut args = env::args().skip(1);
        let mut theme_name = "orbt".to_string();
        let mut all_themes = false;
        let mut no_animation = false;
        let mut slow = false;
        let mut full = false;
        let mut tps: u64 = 1000 / TICK_MS;

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--theme" => {
                    if let Some(name) = args.next() {
                        theme_name = name;
                    }
                }
                "--all-themes" => all_themes = true,
                "--no-animation" => no_animation = true,
                "--slow" => slow = true,
                "--full" => full = true,
                "--tps" => {
                    if let Some(n) = args.next() {
                        if let Ok(n) = n.parse::<u64>() {
                            if n > 0 {
                                tps = n;
                            }
                        }
                    }
                }
                "--help" | "-h" => {
                    Self::print_usage();
                    std::process::exit(0);
                }
                _ => {
                    eprintln!("Unknown flag: {}", arg);
                    Self::print_usage();
                    std::process::exit(2);
                }
            }
        }

        if !ALL_THEMES.contains(&theme_name.as_str()) {
            eprintln!("Unknown theme: {}", theme_name);
            eprintln!("Available: {:?}", ALL_THEMES);
            std::process::exit(2);
        }

        CliArgs {
            theme_name,
            all_themes,
            no_animation,
            slow,
            full,
            tps,
        }
    }

    fn print_usage() {
        println!("Usage: dial_preview [options]");
        println!("Options:");
        println!("  --theme <name>     Set theme (default: orbt)");
        println!("  --all-themes      Show all theme comparison");
        println!("  --no-animation    Freeze to degradation frames");
        println!("  --slow             Step one frame per 400ms");
        println!("  --full             Show all sections even if they don't fit");
        println!(
            "  --tps <n>          Ticks per second (default: {})",
            1000 / TICK_MS
        );
        println!("  --help, -h         Show this help");
    }
}

struct TerminalGuard;

impl TerminalGuard {
    fn new() -> Self {
        crossterm::terminal::enable_raw_mode().ok();
        print!("\x1b[?1049h\x1b[?25l");
        std::io::stdout().flush().ok();
        TerminalGuard
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        crossterm::terminal::disable_raw_mode().ok();
        print!("\x1b[0m\x1b[?25h\x1b[?1049l");
        std::io::stdout().flush().ok();
    }
}

const DIAL_COLS: usize = 3;
const COL_GAP: usize = 3;

fn draw_row(f: &DialFrame, r: usize) -> String {
    let mut s = String::new();
    for c in 0..DIAL_COLS {
        s.push_str(&ansi_fg(f.row_colors[r][c]));
        s.push_str(f.rows[r][c]);
    }
    s.push_str(&ansi_reset());
    s
}

fn inline(f: &DialFrame) -> String {
    (0..3).map(|r| draw_row(f, r)).collect::<Vec<_>>().join(" ")
}

fn clamp_line(line: &str, max_visible: usize) -> String {
    let mut out = String::new();
    let mut vis = 0usize;
    let mut esc = false;
    let mut csi = false;
    for ch in line.chars() {
        if esc {
            esc = false;
            out.push(ch);
            if ch == '[' {
                csi = true;
            }
            continue;
        }
        if csi {
            out.push(ch);
            if ('\u{40}'..='\u{7e}').contains(&ch) {
                csi = false;
            }
            continue;
        }
        if ch == '\u{1b}' {
            esc = true;
            out.push(ch);
            continue;
        }
        if vis >= max_visible {
            out.push_str(&ansi_reset());
            return out;
        }
        out.push(ch);
        vis += 1;
    }
    out.push_str(&ansi_reset());
    out
}

fn heading(text: &str) -> String {
    format!("{}{}\x1b[0m", ansi_fg(fg_secondary()), text)
}

fn heading_dim(text: &str) -> String {
    format!("{}{}{}\x1b[0m", ansi_dim(), ansi_fg(fg_secondary()), text)
}

struct Section {
    name: &'static str,
    lines: Vec<String>,
    optional: bool,
}

fn compose(sections: &[Section], cols: u16, rows: u16, full: bool) -> Vec<String> {
    let width = (cols as usize).saturating_sub(1).max(20);
    let budget = (rows as usize).saturating_sub(2);
    let total: usize = sections.iter().map(|s| s.lines.len()).sum();

    let mut kept: Vec<&Section> = Vec::new();
    let mut dropped: Vec<&str> = Vec::new();
    let mut used = 0usize;

    for s in sections.iter().filter(|s| !s.optional) {
        used += s.lines.len();
        kept.push(s);
    }
    for s in sections.iter().filter(|s| s.optional) {
        if full || used + s.lines.len() <= budget {
            used += s.lines.len();
            kept.push(s);
        } else {
            dropped.push(s.name);
        }
    }

    let mut out: Vec<String> = Vec::new();
    for s in &kept {
        out.extend(s.lines.iter().map(|l| clamp_line(l, width)));
    }
    if !dropped.is_empty() {
        out.push(clamp_line(
            &format!(
                "{}{}hidden: {} — need {} rows, have {} (--full to force)",
                ansi_dim(),
                ansi_fg(fg_muted()),
                dropped.join(", "),
                total,
                rows
            ),
            width,
        ));
    }
    out
}

fn header_section(
    theme: &str,
    no_animation: bool,
    slow: bool,
    tps: u64,
    cols: u16,
    rows: u16,
) -> Section {
    let title = format!(
        "{}{}SATELLITE DIAL PREVIEW\x1b[0m",
        ansi_bold(),
        ansi_fg(fg_primary())
    );
    let mut flags: Vec<String> = Vec::new();
    if no_animation {
        flags.push(format!("{}--no-animation", ansi_fg(accent_error())));
    }
    if slow {
        flags.push(format!("{}--slow", ansi_fg(accent_idle())));
    }
    let flag_text = if flags.is_empty() {
        String::new()
    } else {
        format!("{}  ", flags.join("  "))
    };
    let meta = format!(
        "theme: {}{}{}  tps={}  term={}x{}",
        ansi_fg(fg_secondary()),
        theme,
        ansi_reset(),
        tps,
        cols,
        rows
    );
    let hint = format!("{}{}[q] quit\x1b[0m", ansi_dim(), ansi_fg(fg_muted()),);
    Section {
        name: "header",
        lines: vec![title, format!("{}{}", meta, flag_text), hint, String::new()],
        optional: false,
    }
}

struct LiveCol {
    label: &'static str,
    color: Color,
    frame: DialFrame,
    tick: u64,
}

fn live_section(tick: u64, no_animation: bool) -> Section {
    let specs: [(&'static str, Color, DialState, u64); 4] = [
        (
            "Working",
            accent(),
            DialState::Working,
            tick % WORKING_PERIOD,
        ),
        (
            "Blocked",
            accent_blocked(),
            DialState::Blocked,
            tick % BLOCKED_PERIOD,
        ),
        ("Idle", accent_idle(), DialState::Idle, 0),
        (
            "Error",
            accent_error(),
            DialState::Error,
            tick % ERROR_PERIOD,
        ),
    ];

    let cols: Vec<LiveCol> = specs
        .iter()
        .map(|(label, color, state, local)| {
            let (frame, _) = DialFrame::render_dial(*state, *local, no_animation);
            LiveCol {
                label,
                color: *color,
                frame,
                tick: *local,
            }
        })
        .collect();

    let widths: Vec<usize> = cols
        .iter()
        .map(|c| c.label.len().max(DIAL_COLS).max(4))
        .collect();

    let mut lines: Vec<String> = vec![heading("LIVE PREVIEW"), String::new()];

    let row_line = |cells: Vec<String>| -> String {
        let mut s = String::new();
        for (i, cell) in cells.iter().enumerate() {
            s.push_str(&pad_end(cell, widths[i]));
            if i + 1 < cells.len() {
                s.push_str(&" ".repeat(COL_GAP));
            }
        }
        s
    };

    lines.push(row_line(
        cols.iter()
            .map(|c| format!("{}{}{}", ansi_fg(c.color), c.label, ansi_reset()))
            .collect(),
    ));
    for r in 0..3 {
        lines.push(row_line(
            cols.iter().map(|c| draw_row(&c.frame, r)).collect(),
        ));
    }
    lines.push(row_line(
        cols.iter()
            .zip(widths.iter())
            .map(|(c, w)| {
                format!(
                    "{}{:<w$}{}",
                    ansi_fg(fg_muted()),
                    format!("t={}", c.tick),
                    ansi_reset(),
                    w = w
                )
            })
            .collect(),
    ));

    Section {
        name: "live",
        lines,
        optional: false,
    }
}

fn port_titlebar_section(working_tick: u64, no_animation: bool, cols: u16) -> Section {
    let (frame, _) = DialFrame::render_dial(DialState::Working, working_tick, no_animation);

    let path = "\u{2026}/api/src/middleware";
    let close = "\u{d7}";
    let name = "claude";

    let rule_w = (cols as usize).saturating_sub(4).max(30);
    let rule = "\u{2500}".repeat(rule_w);

    let right_block = DIAL_COLS + 2 + name.len() + 2 + close.chars().count();
    let dial_x = rule_w.saturating_sub(right_block).max(DIAL_COLS + 2);
    let path_room = dial_x.saturating_sub(1).max(4);

    let indent = " ".repeat(dial_x);
    let row_path = format!(
        "{}{}{}",
        ansi_fg(fg_muted()),
        &pad_end(&truncate_left(path, path_room), path_room),
        ansi_reset()
    );

    Section {
        name: "port",
        lines: vec![
            String::new(),
            heading("PORT TITLE BAR (insertion point — dial keeps its 3 rows)"),
            format!("{}{}{}", ansi_fg(border_dim()), rule, ansi_reset()),
            format!("{}{}", indent, draw_row(&frame, 0)),
            format!(
                "{}{}  {}{}  {}{}{}",
                row_path,
                " ".repeat(dial_x.saturating_sub(path_room)),
                draw_row(&frame, 1),
                ansi_reset(),
                ansi_fg(accent()),
                name,
                format!("  {}{}{}", ansi_fg(fg_muted()), close, ansi_reset())
            ),
            format!("{}{}", indent, draw_row(&frame, 2)),
            format!("{}{}{}", ansi_fg(border_dim()), rule, ansi_reset()),
        ],
        optional: false,
    }
}

fn done_section() -> Section {
    Section {
        name: "done",
        lines: vec![
            heading("DONE (no orbit — renders as a single-cell glyph)"),
            format!(
                "  {}{}\u{2713}{}  fg_muted",
                ansi_fg(fg_muted()),
                ansi_dim(),
                ansi_reset()
            ),
        ],
        optional: false,
    }
}

fn slots_section() -> Section {
    let (n, _) = DialFrame::render_dial(DialState::Working, 0, false);
    let (e, _) = DialFrame::render_dial(DialState::Working, 12, false);
    let (s, _) = DialFrame::render_dial(DialState::Working, 24, false);
    let (w, _) = DialFrame::render_dial(DialState::Working, 36, false);

    let cell = |tag: &str, f: &DialFrame| {
        format!(
            "{}{}{}  {}",
            ansi_fg(fg_muted()),
            tag,
            ansi_reset(),
            inline(f)
        )
    };

    Section {
        name: "slots",
        lines: vec![
            String::new(),
            heading_dim("SLOT REFERENCE (dimmed)"),
            format!("{}   {}", cell("N", &n), cell("S", &s)),
            format!("{}   {}", cell("E", &e), cell("W", &w)),
        ],
        optional: true,
    }
}

fn revolution_section() -> Section {
    let specs: [(u64, u64, &str); 4] = [
        (0, 11, "N"),
        (12, 23, "E"),
        (24, 35, "S shade"),
        (36, 47, "W"),
    ];
    let mut lines = vec![
        String::new(),
        heading_dim("WORKING REVOLUTION — 48 ticks, 12 per frame, 40% trail at previous slot"),
    ];
    for (from, to, tag) in specs {
        let (frame, _) = DialFrame::render_dial(DialState::Working, from, false);
        let note = if from == 24 {
            format!(
                "   {}\u{2190} satellite in shadow slot, hollow + fg_muted",
                ansi_dim()
            )
        } else if from == 36 {
            format!("   {}\u{2190} trail also in shadow slot", ansi_dim())
        } else {
            String::new()
        };
        lines.push(format!(
            "{}{:<6}{}  {:<10}  {}{}",
            ansi_fg(fg_muted()),
            format!("t={}-{}", from, to),
            ansi_reset(),
            tag,
            inline(&frame),
            note
        ));
    }
    Section {
        name: "revolution",
        lines,
        optional: true,
    }
}

fn tokens_section() -> Section {
    let rows: [(&str, Color); 8] = [
        ("accent", accent()),
        ("accent_idle", accent_idle()),
        ("accent_blocked", accent_blocked()),
        ("accent_error", accent_error()),
        ("fg_muted", fg_muted()),
        ("border_dim", border_dim()),
        ("bg_primary", bg_primary()),
        ("bg_card", bg_card()),
    ];
    let mut lines = vec![
        String::new(),
        heading_dim("TOKEN RESOLUTION (colors come from the theme, never hardcoded)"),
        format!(
            "{}{:<16}{:>18}  {:>10}\x1b[0m",
            ansi_dim(),
            "token",
            "rgb",
            "hex",
        ),
    ];
    for (name, color) in rows {
        let (r, g, b) = color_to_rgb(color);
        let hex_colored = format!("{}{}{}", ansi_fg(color), color_to_hex(color), ansi_reset());
        lines.push(format!(
            "{}{:<16}{:>18}  {}",
            ansi_fg(fg_secondary()),
            name,
            format!("{},{},{}", r, g, b),
            pad_start(&hex_colored, 10)
        ));
    }
    Section {
        name: "tokens",
        lines,
        optional: true,
    }
}

fn all_themes_section() -> Section {
    let mut lines = vec![
        heading("ALL THEMES — Working dial, degradation frame"),
        String::new(),
    ];
    let original = current_theme_name();
    for name in ALL_THEMES {
        set_theme(name);
        let (frame, _) = DialFrame::render_dial(DialState::Working, 0, true);
        lines.push(format!(
            "{:>14}  {}  {}{}{}",
            name,
            inline(&frame),
            ansi_fg(fg_muted()),
            color_to_hex(accent()),
            ansi_reset()
        ));
    }
    set_theme(&original);
    Section {
        name: "all-themes",
        lines,
        optional: false,
    }
}

fn draw_all_themes_strip() -> String {
    let mut out = String::new();
    out.push_str("\x1b[2J");
    for line in all_themes_section().lines {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str(&ansi_reset());
    out
}

fn color_to_hex(c: Color) -> String {
    let (r, g, b) = color_to_rgb(c);
    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

fn current_theme_name() -> String {
    std::env::var("ORBT_PREVIEW_THEME").unwrap_or_else(|_| "orbt".to_string())
}

fn truncate_left(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    let tail: String = chars[chars.len() - keep..].iter().collect();
    format!("\u{2026}{}", tail)
}

fn main() {
    let args = CliArgs::parse();
    set_theme(&args.theme_name);
    std::env::set_var("ORBT_PREVIEW_THEME", &args.theme_name);

    if args.all_themes {
        print!("{}", draw_all_themes_strip());
        std::io::stdout().flush().ok();
        return;
    }

    let _guard = TerminalGuard::new();

    let tick_ms = 1000 / args.tps.max(1);
    let start = Instant::now();
    let mut last_tick: u64 = 0;

    loop {
        if let Ok(true) = crossterm::event::poll(std::time::Duration::from_millis(0)) {
            if let Ok(crossterm::event::Event::Key(key)) = crossterm::event::read() {
                let ctrl_c = key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL)
                    && key.code == crossterm::event::KeyCode::Char('c');
                if ctrl_c
                    || matches!(
                        key.code,
                        crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Esc
                    )
                {
                    break;
                }
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let raw = elapsed_ms / tick_ms;
        let tick = if args.slow { raw / 25 } else { raw };
        let step = if args.slow { 25 } else { 1 };

        let (cols, rows) = crossterm::terminal::size().unwrap_or((100, 46));

        if tick != last_tick {
            last_tick = tick;
            let sections = vec![
                header_section(
                    &args.theme_name,
                    args.no_animation,
                    args.slow,
                    args.tps,
                    cols,
                    rows,
                ),
                live_section(tick, args.no_animation),
                port_titlebar_section(tick, args.no_animation, cols),
                done_section(),
                slots_section(),
                revolution_section(),
                tokens_section(),
            ];
            let lines = compose(&sections, cols, rows, args.full);

            print!("\x1b[H");
            for l in &lines {
                print!("{}\r\n", l);
            }
            std::io::stdout().flush().ok();
        }

        let target_ms = (last_tick + step) * tick_ms;
        let now_ms = start.elapsed().as_millis() as u64;
        if target_ms > now_ms {
            std::thread::sleep(std::time::Duration::from_millis(
                (target_ms - now_ms).min(tick_ms),
            ));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frame_dimensions() {
        let states = [
            (DialState::Working, 0),
            (DialState::Working, 12),
            (DialState::Working, 24),
            (DialState::Working, 36),
            (DialState::Blocked, 0),
            (DialState::Blocked, 24),
            (DialState::Idle, 0),
            (DialState::Error, 0),
            (DialState::Error, 30),
        ];

        for (state, tick) in states {
            let (frame, _) = DialFrame::render_dial(state, tick, false);
            for row in 0..3 {
                let row_str: String = frame.rows[row].iter().map(|s| *s).collect();
                assert_eq!(
                    row_str.chars().count(),
                    3,
                    "Row {} width != 3 for {:?} at tick {}",
                    row,
                    state,
                    tick
                );
            }
        }
    }

    #[test]
    #[should_panic(expected = "N slot must be solid")]
    fn test_n_slot_rejects_hollow() {
        let _ = DialFrame::slot_content(Slot::N, SatelliteGlyph::Hollow, fg_primary());
    }

    #[test]
    #[should_panic(expected = "N slot must be solid")]
    fn test_n_slot_rejects_circle() {
        let _ = DialFrame::slot_content(Slot::N, SatelliteGlyph::Circle, fg_primary());
    }

    #[test]
    fn test_n_slot_safe_in_production_path() {
        for tick in 0..48 {
            let (frame, _) = DialFrame::render_dial(DialState::Working, tick, false);
            assert_ne!(frame.rows[0][1], "\u{25cb}");
            assert_ne!(frame.rows[0][1], "\u{25cc}");
        }
        for tick in 0..60 {
            let (frame, _) = DialFrame::render_dial(DialState::Error, tick, false);
            assert_ne!(frame.rows[0][1], "\u{25cb}");
            assert_ne!(frame.rows[0][1], "\u{25cc}");
        }
    }

    #[test]
    fn test_blocked_blinks() {
        for tick in [0, 12, 23] {
            let (frame, _) = DialFrame::render_dial(DialState::Blocked, tick, false);
            for r in 0..3 {
                for c in 0..3 {
                    if (r, c) == (2, 1) {
                        continue;
                    }
                    assert_eq!(
                        frame.row_colors[r][c],
                        accent_blocked(),
                        "tick {} ring should be accent_blocked bright",
                        tick
                    );
                }
            }
        }
        for tick in [24, 36, 47] {
            let (frame, _) = DialFrame::render_dial(DialState::Blocked, tick, false);
            for r in 0..3 {
                for c in 0..3 {
                    if (r, c) == (2, 1) {
                        continue;
                    }
                    assert_eq!(
                        frame.row_colors[r][c],
                        border_dim(),
                        "tick {} ring should be border_dim",
                        tick
                    );
                }
            }
        }
    }

    #[test]
    fn test_working_trail_dimmed_in_shadow_slot() {
        let (frame, _) = DialFrame::render_dial(DialState::Working, 36, false);
        let (trail_row, trail_col) = DialFrame::slot_row_col(Slot::S);
        assert_eq!(
            frame.row_colors[trail_row][trail_col],
            fg_muted(),
            "trail in S slot at tick 36 must be fg_muted"
        );
        let (frame2, _) = DialFrame::render_dial(DialState::Working, 12, false);
        let (trail_row2, trail_col2) = DialFrame::slot_row_col(Slot::N);
        assert_eq!(
            frame2.row_colors[trail_row2][trail_col2],
            lerp_rgb(accent(), bg_primary(), 0.4),
            "trail in N slot at tick 12 must be trail_color"
        );
    }

    #[test]
    fn test_slot_frame_mapping() {
        let (n_frame, _) = DialFrame::render_dial(DialState::Working, 0, false);
        assert_eq!(n_frame.rows[0][1], "\u{25cf}");
        assert_eq!(n_frame.rows[1][0], "\u{2502}");
        assert_eq!(n_frame.rows[1][2], "\u{2502}");
        assert_eq!(n_frame.rows[2][1], "\u{2500}");

        let (e_frame, _) = DialFrame::render_dial(DialState::Working, 12, false);
        assert_eq!(e_frame.rows[1][2], "\u{25cf}");

        let (s_frame, _) = DialFrame::render_dial(DialState::Working, 24, false);
        assert_eq!(s_frame.rows[2][1], "\u{25cc}");

        let (w_frame, _) = DialFrame::render_dial(DialState::Idle, 0, false);
        assert_eq!(w_frame.rows[1][0], "\u{25cb}");
    }

    #[test]
    fn test_working_revolution_distinct_frames() {
        let frames: Vec<(DialFrame, Option<Slot>)> = (0..48)
            .step_by(12)
            .map(|t| DialFrame::render_dial(DialState::Working, t, false))
            .collect();

        assert_eq!(frames.len(), 4);

        let f0 = &frames[0].0.rows;
        let f1 = &frames[1].0.rows;
        let f2 = &frames[2].0.rows;
        let f3 = &frames[3].0.rows;

        assert_ne!(f0, f1);
        assert_ne!(f1, f2);
        assert_ne!(f2, f3);
        assert_ne!(f3, f0);
    }

    #[test]
    fn test_working_returns_after_48_ticks() {
        let (frame0, _) = DialFrame::render_dial(DialState::Working, 0, false);
        let (frame48, _) = DialFrame::render_dial(DialState::Working, 48, false);

        assert_eq!(frame0.rows, frame48.rows);
        assert_eq!(frame0.row_colors, frame48.row_colors);
    }

    #[test]
    fn test_error_no_shadow_rule() {
        let (n_frame, _) = DialFrame::render_dial(DialState::Error, 0, false);
        assert_eq!(n_frame.rows[0][1], "\u{25cf}");

        let (s_frame, _) = DialFrame::render_dial(DialState::Error, 30, false);
        assert_eq!(s_frame.rows[2][1], "\u{25cf}");
    }

    #[test]
    fn test_blocked_shadow_rule() {
        let (frame, _) = DialFrame::render_dial(DialState::Blocked, 0, false);
        assert_eq!(frame.rows[2][1], "\u{25cc}");
    }

    #[test]
    fn test_working_revolution_exact_frames() {
        const EXPECTED: [(u64, &str); 4] = [
            (
                0,
                "\u{256d}\u{25cf}\u{256e}\u{2502}\u{20}\u{2502}\u{2570}\u{2500}\u{256f}",
            ),
            (
                12,
                "\u{256d}\u{b7}\u{256e}\u{2502}\u{20}\u{25cf}\u{2570}\u{2500}\u{256f}",
            ),
            (
                24,
                "\u{256d}\u{2500}\u{256e}\u{2502}\u{20}\u{b7}\u{2570}\u{25cc}\u{256f}",
            ),
            (
                36,
                "\u{256d}\u{2500}\u{256e}\u{25cf}\u{20}\u{2502}\u{2570}\u{b7}\u{256f}",
            ),
        ];
        for (tick, want) in EXPECTED {
            let (frame, _) = DialFrame::render_dial(DialState::Working, tick, false);
            let got: String = frame.rows.iter().flatten().copied().collect();
            assert_eq!(got, want, "Working frame at tick {} mismatch", tick);
            assert_eq!(got.chars().count(), 9, "frame must be exactly 3x3");
        }
    }

    #[test]
    fn test_vwidth_basic() {
        assert_eq!(vwidth("hello"), 5);
        assert_eq!(vwidth(""), 0);
        assert_eq!(vwidth("\x1b[38;2;255;0;0mhello"), 5);
        assert_eq!(vwidth("\x1b[0m"), 0);
    }

    #[test]
    fn test_pad_end_start() {
        assert_eq!(vwidth(&pad_end("hi", 5)), 5);
        assert_eq!(vwidth(&pad_start("hi", 5)), 5);
        assert_eq!(pad_end("hello", 3), "hello");
        assert_eq!(pad_start("hello", 3), "hello");
    }
}
