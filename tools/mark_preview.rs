use std::io::Write;
use std::time::Instant;

const W: usize = 64;
const H: usize = 22;
const CX: f64 = 16.0;
const CY: f64 = 8.5;
const RX: f64 = 9.0;
const RY: f64 = 4.6;
const FRAMES: u64 = 12;

const ACCENT: &str = "\x1b[38;2;187;154;247m";
const DIM: &str = "\x1b[38;2;70;75;105m";
const TXT: &str = "\x1b[38;2;148;156;187m";
const MUTE: &str = "\x1b[38;2;90;95;128m";

fn fill_level(d: f64) -> char {
    if d < 0.55 {
        '\u{2588}'
    } else if d < 0.80 {
        '\u{2593}'
    } else if d < 0.97 {
        '\u{2592}'
    } else {
        '\u{2591}'
    }
}

fn main() {
    print!("\x1b[?1049h\x1b[?25l\x1b[2J");
    let _ = crossterm::terminal::enable_raw_mode();

    let start = Instant::now();
    let mut last = u64::MAX;
    loop {
        if let Ok(true) = crossterm::event::poll(std::time::Duration::from_millis(0)) {
            if let Ok(crossterm::event::Event::Key(k)) = crossterm::event::read() {
                if matches!(k.code, crossterm::event::KeyCode::Char('q') | crossterm::event::KeyCode::Esc) {
                    break;
                }
            }
        }

        let step = start.elapsed().as_millis() as u64 / 95;
        if step == last {
            std::thread::sleep(std::time::Duration::from_millis(8));
            continue;
        }
        last = step;

        let th = 2.0 * std::f64::consts::PI * (step % FRAMES) as f64 / FRAMES as f64;
        let (s, co) = th.sin_cos();

        print!("\x1b[H\x1b[2J");
        print!("{}  \x1b[1mO R B I T\x1b[0m\r\n", TXT);
        print!("{}  \u{300c}\u{8680}\u{300d} \u{7ec8}\u{7ed3}\u{7ebf}\u{65cb}\u{8f6c} \u{2014} \u{8f6c}\u{4e00}\u{5708} = \u{536b}\u{661f}\u{7ed5}\u{884c}\u{4e00}\u{5708} = \u{4e00}\u{4e2a}\u{4efb}\u{52a1}\u{5468}\u{671f}\r\n\r\n", MUTE);

        for r in 0..H {
            let mut line = String::new();
            for c in 0..W {
                let dx = (c as f64 + 0.5 - CX) / RX;
                let dy = (r as f64 + 0.5 - CY) / RY;
                let d = (dx * dx + dy * dy).sqrt();
                if d > 1.0 {
                    line.push(' ');
                    continue;
                }
                let ch = fill_level(d);
                let night = (c as f64 + 0.5 - CX) * co + (r as f64 + 0.5 - CY) * s < 0.0;
                let col = if night { DIM } else { ACCENT };
                line.push_str(col);
                line.push(ch);
                line.push_str("\x1b[0m");
            }
            print!("{}\r\n", line.trim_end());
        }

        print!("\r\n  {}size ladder{}\r\n", MUTE, TXT);
        print!("    {}1-cell   {}{}\u{25d2}\x1b[0m   {}\u{25d3} \u{25d1} \u{25d0}\r\n", MUTE, ACCENT, MUTE, MUTE);
        print!("    {}3-cell   {}\u{256d}\u{2500}\u{256e}\x1b[0m {}{}\u{25cf} \u{2502}\x1b[0m {}\u{2570}\u{2500}\u{256f}\x1b[0m\r\n", MUTE, TXT, ACCENT, TXT, MUTE);
        print!("    {}5-cell   {}\u{256d}\u{2500}\u{2500}\u{2500}\u{256e}\x1b[0m  {}{}\u{25cf} \u{2502} \u{2502}\x1b[0m  {}\u{2570}\u{2500}\u{2500}\u{2500}\u{256f}\x1b[0m\r\n", MUTE, TXT, ACCENT, TXT, MUTE);
        print!("\r\n  {}[q] quit\x1b[0m", MUTE);
        let _ = std::io::stdout().flush();
    }

    let _ = crossterm::terminal::disable_raw_mode();
    print!("\x1b[0m\x1b[?25h\x1b[?1049l");
    let _ = std::io::stdout().flush();
}
