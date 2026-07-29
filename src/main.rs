//! isotopes — the chart of the nuclides in a terminal.
//!
//! Neutrons across, protons up, one cell per nuclide, 3,386 of them.
//! The valley of stability is not drawn: it falls out of the data, the
//! way the main sequence falls out of a Hertzsprung-Russell diagram.
//!
//! Walk it with the arrow keys, press Enter on uranium-238 and follow it
//! down fourteen steps to lead-206.

mod canvas;
mod data;

use crust::style;
use crust::{seq, Crust, Cursor, Input, Pane, Popup};
use data::{rgb_for, table, Decay, MODES};
use std::io::Write;

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Room for the element symbol down the left edge.
const AXIS_W: u16 = 4;
/// Rows the readout under the chart takes.
const DETAIL_H: u16 = 7;
const ASK_RGB: (u8, u8, u8) = (255, 200, 120);
const ERR_RGB: (u8, u8, u8) = (255, 120, 100);

#[derive(PartialEq, Clone, Copy)]
enum View {
    Chart,
    Overview,
}

struct App {
    /// Where the cursor is, as a real nuclide.
    z: u32,
    n: u32,
    mode: usize,
    view: View,
    /// Top-left of the visible window into the chart.
    top_z: u32,
    left_n: u32,
    /// The window the last frame was drawn with. When the chart slides,
    /// every cell on screen means something different, and anything the
    /// new frame does not cover has to go.
    drawn_at: Option<(u32, u32, u16, u16)>,
    chat: Vec<(String, String)>,
    status: Option<(String, (u8, u8, u8))>,
}

fn main() {
    if std::env::args().skip(1).any(|a| a == "-h" || a == "--help") {
        println!("isotopes — the chart of the nuclides (Fe2O3 suite)");
        println!();
        println!("Usage: isotopes [NUCLIDE]");
        println!();
        println!("  NUCLIDE   start on this one, e.g. U-238, fe56, 14C");
        println!();
        println!("3,386 nuclides: half-lives, decay modes and branchings, spin and");
        println!("parity, binding energy, mass excess, natural abundance. Enter walks");
        println!("a decay chain to the stable end. Data from the IAEA, offline.");
        return;
    }
    if std::env::args().skip(1).any(|a| a == "-v" || a == "--version") {
        println!("isotopes {VERSION}");
        return;
    }

    let t = table();
    let start = std::env::args()
        .nth(1)
        .and_then(|q| t.find(&q))
        .unwrap_or_else(|| t.index(92, 146).unwrap_or(0));
    let mut app = App {
        z: t.all[start].z,
        n: t.all[start].n,
        mode: 0,
        view: View::Chart,
        top_z: 0,
        left_n: 0,
        drawn_at: None,
        chat: Vec::new(),
        status: None,
    };

    Crust::init();
    Crust::set_app_identity("Isotopes");
    // Whatever was on this terminal before is not ours to keep.
    Crust::clear_screen();
    let (mut cols, mut rows) = Crust::terminal_size();
    let mut footer = Pane::new(1, rows, cols, 1, 250, 236);
    footer.scroll = false;

    (cols, rows) = draw(&mut app, &mut footer);

    loop {
        let Some(key) = Input::getchr(None) else { continue };
        match key.as_str() {
            "q" | "Q" => break,
            "RIGHT" | "l" => app.step_n(1),
            "LEFT" | "h" => app.step_n(-1),
            "UP" | "k" => app.step_z(1),
            "DOWN" | "j" => app.step_z(-1),
            "PgUP" | "K" => app.step_z(10),
            "PgDOWN" | "J" => app.step_z(-10),
            "HOME" => app.jump_edge(false),
            "END" => app.jump_edge(true),
            "g" => app.goto(1, 0),
            "G" => app.goto(table().z_max, 0),
            "TAB" => app.next_stable(1),
            "S-TAB" => app.next_stable(-1),
            "z" => {
                app.view = if app.view == View::Chart { View::Overview } else { View::Chart };
                app.drawn_at = None;
                // The two views cover different cells; start the new one
                // on a clean screen.
                Crust::clear_screen();
            }
            "1" | "2" | "3" | "4" | "5" => {
                app.mode = key.parse::<usize>().unwrap_or(1) - 1;
            }
            "m" => app.mode = (app.mode + 1) % MODES.len(),
            "M" => app.mode = (app.mode + MODES.len() - 1) % MODES.len(),
            "ENTER" => {
                show_chain(&app, cols, rows);
                Crust::clear_screen();
            }
            "/" => {
                let q = footer.ask_or_cancel("find: ", "");
                print!("{}", Cursor::hide_seq());
                std::io::stdout().flush().ok();
                if let Some(q) = q {
                    match table().find(&q) {
                        Some(i) => {
                            let nuc = &table().all[i];
                            app.goto(nuc.z, nuc.n);
                        }
                        None => app.say(&format!("no nuclide called {}", q.trim()), ERR_RGB),
                    }
                }
            }
            "c" => {
                let q = footer.ask_or_cancel("ask claude: ", "");
                print!("{}", Cursor::hide_seq());
                std::io::stdout().flush().ok();
                if let Some(q) = q {
                    if !q.trim().is_empty() {
                        footer.say(&style::rgb(" asking claude…", Some(ASK_RGB), None, ""));
                        std::io::stdout().flush().ok();
                        match ask_claude(&app, q.trim()) {
                            Ok(a) if !a.is_empty() => {
                                app.chat.push((q.trim().to_string(), a.clone()));
                                Crust::clear_screen();
                                let w = cols.saturating_sub(8).min(96);
                                let h = rows.saturating_sub(4).min(34);
                                let mut p = Popup::centered(w, h, 252, 234);
                                p.view(&format!(
                                    "{}\n\n{}\n\n{}",
                                    style::rgb(&app.cur().name(), Some(ASK_RGB), None, "b"),
                                    style::dim(q.trim()),
                                    a
                                ));
                                Crust::clear_screen();
                            }
                            Ok(_) => app.say("claude returned nothing", ERR_RGB),
                            Err(e) => app.say(&format!("claude: {e}"), ERR_RGB),
                        }
                    }
                }
            }
            "e" => match export(&app) {
                Ok(p) => app.say(&format!("wrote {p}"), (140, 220, 140)),
                Err(e) => app.say(&format!("export: {e}"), ERR_RGB),
            },
            "r" | "C-L" => {
                Crust::clear_screen();
                app.drawn_at = None;
            }
            "?" => {
                show_help(cols, rows);
                Crust::clear_screen();
            }
            // Handled in draw(), which re-reads the size every frame.
            "RESIZE" => Crust::clear_screen(),
            _ => {}
        }
        (cols, rows) = draw(&mut app, &mut footer);
    }

    Crust::cleanup();
    Crust::clear_screen();
}

impl App {
    fn cur(&self) -> &'static data::Nuclide {
        let t = table();
        t.get(self.z, self.n).unwrap_or(&t.all[0])
    }

    fn say(&mut self, msg: &str, rgb: (u8, u8, u8)) {
        self.status = Some((msg.to_string(), rgb));
    }

    fn goto(&mut self, z: u32, n: u32) {
        let t = table();
        if t.get(z, n).is_some() {
            self.z = z;
            self.n = n;
            return;
        }
        // Land on the nearest nuclide of that element instead of nowhere.
        if let Some(&i) = t
            .isotopes_of(z)
            .iter()
            .min_by_key(|&&i| (t.all[i].n as i32 - n as i32).abs())
        {
            self.z = t.all[i].z;
            self.n = t.all[i].n;
        }
    }

    /// Along an element's isotopes, skipping the gaps.
    fn step_n(&mut self, dir: i32) {
        let t = table();
        let mut n = self.n as i32;
        for _ in 0..(t.n_max + 2) {
            n += dir;
            if n < 0 || n > t.n_max as i32 {
                return;
            }
            if t.get(self.z, n as u32).is_some() {
                self.n = n as u32;
                return;
            }
        }
    }

    /// Up or down an element, keeping as close to the same N as exists.
    fn step_z(&mut self, dir: i32) {
        let t = table();
        let mut z = self.z as i32;
        for _ in 0..(t.z_max + 2) {
            z += dir.signum();
            if z < 0 || z > t.z_max as i32 {
                return;
            }
            if !t.isotopes_of(z as u32).is_empty() {
                let want = self.n;
                self.goto(z as u32, want);
                if dir.abs() > 1 {
                    self.step_z(dir - dir.signum());
                }
                return;
            }
        }
    }

    fn jump_edge(&mut self, last: bool) {
        let t = table();
        let iso = t.isotopes_of(self.z);
        let pick = if last { iso.last() } else { iso.first() };
        if let Some(&i) = pick {
            self.n = t.all[i].n;
        }
    }

    /// The next stable nuclide up or down the chart: a quick way to walk
    /// the valley floor.
    fn next_stable(&mut self, dir: i32) {
        let t = table();
        let mut z = self.z as i32;
        for _ in 0..(t.z_max + 2) {
            z += dir;
            if z < 1 || z > t.z_max as i32 {
                return;
            }
            if let Some(&i) = t
                .isotopes_of(z as u32)
                .iter()
                .find(|&&i| t.all[i].is_stable())
            {
                self.z = t.all[i].z;
                self.n = t.all[i].n;
                return;
            }
        }
    }
}

// ─────────────────────────── drawing ─────────────────────────────────

fn draw(app: &mut App, footer: &mut Pane) -> (u16, u16) {
    // Ask the terminal how big it is on every frame rather than trusting
    // the size we were told at startup. A window manager that resizes
    // the window after launch, or a terminal that does not send the
    // signal, otherwise leaves the app drawing into part of the screen
    // and the rest holding whatever was there before.
    let (cols, rows) = Crust::terminal_size();
    if cols != footer.w || rows != footer.y {
        footer.w = cols;
        footer.y = rows;
        footer.full_refresh();
        Crust::clear_screen();
    }
    let chart_h = rows.saturating_sub(2 + DETAIL_H + 1);
    if app.view == View::Chart {
        scroll_to(app, cols, chart_h);
        // Scrolling shifts every cell. Wipe first: a repaint alone
        // relies on the terminal clearing what the new frame overwrites
        // with blanks, and one that does not leaves a trail.
        let now = (app.top_z, app.left_n, cols, rows);
        if app.drawn_at.map(|d| d != now).unwrap_or(true) {
            Crust::clear_screen();
            app.drawn_at = Some(now);
        }
    }
    if app.view == View::Overview {
        draw_header(app, cols);
        canvas::overview(app.z, app.n, app.mode, 1, 2, cols, chart_h + 1);
    } else {
        draw_header(app, cols);
        draw_chart(app, cols, chart_h);
    }
    draw_detail(app, cols, rows);
    footer.say(&keys_line(app));
    print!("{}", Cursor::hide_seq());
    std::io::stdout().flush().ok();
    (cols, rows)
}

/// Keep the cursor inside the window, with a margin so it never sits on
/// the very edge.
fn scroll_to(app: &mut App, cols: u16, chart_h: u16) {
    let t = table();
    let vis_n = cols.saturating_sub(AXIS_W) as u32;
    let vis_z = chart_h as u32;
    let margin_n = (vis_n / 6).max(2);
    let margin_z = (vis_z / 6).max(2);

    if app.n < app.left_n + margin_n {
        app.left_n = app.n.saturating_sub(margin_n);
    }
    if app.n + margin_n >= app.left_n + vis_n {
        app.left_n = (app.n + margin_n + 1).saturating_sub(vis_n);
    }
    // top_z is the highest Z on screen; the chart counts down from it.
    if app.z + margin_z > app.top_z {
        app.top_z = (app.z + margin_z).min(t.z_max);
    }
    if app.z + vis_z < app.top_z + margin_z {
        app.top_z = (app.z + vis_z).saturating_sub(margin_z);
    }
    app.top_z = app.top_z.max(vis_z.saturating_sub(1)).min(t.z_max);
}

fn draw_header(app: &App, cols: u16) {
    const BAR: (u8, u8, u8) = (38, 38, 38);
    let n = app.cur();
    let bg = style::set_bg_rgb(BAR.0, BAR.1, BAR.2);
    // Every style helper closes with a reset, which drops the bar's
    // background half way along the row. Re-assert it after each one.
    let armed = |s: &str| s.replace(style::RESET, &format!("{}{}", style::RESET, bg));

    let left = format!(
        " {}  {}  Z {} · N {} · A {} ",
        style::rgb("isotopes", Some((247, 76, 0)), None, "b"),
        style::rgb(&n.name(), Some((255, 220, 140)), None, "b"),
        n.z,
        n.n,
        n.a()
    );
    let right = format!("{}  ·  {} nuclides ", MODES[app.mode], table().all.len());
    let pad = (cols as usize)
        .saturating_sub(crust::display_width(&left) + crust::display_width(&right));
    print!(
        "{}{}{}{}{}{}{}",
        Cursor::at(1, 1),
        bg,
        armed(&left),
        " ".repeat(pad),
        armed(&style::dim(&right)),
        style::RESET,
        seq::ERASE_EOL
    );
}

fn draw_chart(app: &App, cols: u16, chart_h: u16) {
    let t = table();
    let vis_n = cols.saturating_sub(AXIS_W) as u32;
    let mut out = String::new();

    for row in 0..chart_h {
        let z = app.top_z as i32 - row as i32;
        out.push_str(&Cursor::at(1, 2 + row));
        if z < 0 {
            out.push_str(seq::ERASE_EOL);
            continue;
        }
        let z = z as u32;
        // The element's symbol down the left edge, from any of its
        // isotopes: they all share it.
        let sym = t
            .isotopes_of(z)
            .first()
            .map(|&i| t.all[i].symbol.clone())
            .unwrap_or_default();
        let axis = format!("{sym:>3} ");
        out.push_str(&style::rgb(
            &axis,
            Some(if z == app.z { (255, 220, 140) } else { (110, 110, 120) }),
            None,
            if z == app.z { "b" } else { "" },
        ));

        let mut cur: Option<(u8, u8, u8)> = None;
        for c in 0..vis_n {
            let n = app.left_n + c;
            let Some(nuc) = t.get(z, n) else {
                out.push_str(style::RESET);
                cur = None;
                out.push(' ');
                continue;
            };
            if z == app.z && n == app.n {
                out.push_str(style::RESET);
                cur = None;
                out.push_str(&style::rgb("◆", Some((255, 255, 255)), None, "b"));
                continue;
            }
            let rgb = rgb_for(nuc, app.mode);
            if cur != Some(rgb) {
                out.push_str(&style::set_fg_rgb(rgb.0, rgb.1, rgb.2));
                cur = Some(rgb);
            }
            out.push('█');
        }
        // Erase the rest of the row rather than blanking it by hand: a
        // row of spaces only covers what this frame knows about, and the
        // frame before it may have reached further right.
        out.push_str(style::RESET);
        out.push_str(seq::ERASE_EOL);
    }

    // The neutron ruler along the bottom.
    out.push_str(&Cursor::at(1, 2 + chart_h));
    let mut ruler = " ".repeat(AXIS_W as usize);
    let mut n = app.left_n;
    while n % 10 != 0 {
        n += 1;
    }
    while n < app.left_n + vis_n {
        let col = (AXIS_W as u32 + n - app.left_n) as usize;
        let label = n.to_string();
        if col + label.len() < cols as usize {
            while ruler.len() < col {
                ruler.push(' ');
            }
            ruler.truncate(col);
            ruler.push_str(&label);
        }
        n += 10;
    }
    out.push_str(&style::dim(ruler.trim_end()));
    out.push_str(seq::ERASE_EOL);
    print!("{out}");
}

fn draw_detail(app: &App, cols: u16, rows: u16) {
    let t = table();
    let nuc = app.cur();
    let y0 = rows.saturating_sub(DETAIL_H);
    let key = |k: &str| style::rgb(k, Some((120, 170, 220)), None, "");
    let val = |v: &str| style::rgb(v, Some((235, 235, 240)), None, "");

    let decays = if nuc.is_stable() {
        style::rgb("stable", Some((245, 245, 245)), None, "b")
    } else if nuc.decays.is_empty() {
        style::dim("decay mode unknown")
    } else {
        nuc.decays
            .iter()
            .map(|(d, code, pct)| {
                let rgb = d.rgb();
                match pct {
                    Some(p) => style::rgb(&format!("{code} {p}%"), Some(rgb), None, ""),
                    None => style::rgb(code, Some(rgb), None, ""),
                }
            })
            .collect::<Vec<_>>()
            .join(&style::dim(" · "))
    };

    let chain = t.chain(t.index(nuc.z, nuc.n).unwrap_or(0));
    let chain_line = if chain.len() < 2 {
        style::dim("no chain: this is where it stops")
    } else {
        let mut s = String::new();
        for (i, &(idx, mode)) in chain.iter().enumerate() {
            if i > 0 {
                s.push_str(&style::rgb(
                    &format!(" →{} ", mode.short()),
                    Some(mode.rgb()),
                    None,
                    "",
                ));
            }
            let n = &t.all[idx];
            let last = i + 1 == chain.len();
            s.push_str(&style::rgb(
                &n.name(),
                Some(if last && n.is_stable() { (245, 245, 245) } else { (200, 200, 210) }),
                None,
                if last { "b" } else { "" },
            ));
            if crust::display_width(&s) > cols as usize * 3 / 4 && !last {
                s.push_str(&style::dim(" … "));
                s.push_str(&style::rgb(
                    &t.all[chain.last().unwrap().0].name(),
                    Some((245, 245, 245)),
                    None,
                    "b",
                ));
                break;
            }
        }
        format!("{s}{}", style::dim(&steps(chain.len() - 1)))
    };

    let half = if nuc.is_stable() {
        style::rgb("stable", Some((245, 245, 245)), None, "b")
    } else {
        format!(
            "{}{}",
            val(&nuc.half_life_pretty()),
            if nuc.half_life_text.trim().is_empty() {
                String::new()
            } else {
                style::dim(&format!("  ({})", nuc.half_life_text))
            }
        )
    };
    let abundance = match nuc.abundance {
        Some(a) => val(&format!("{a}%")),
        None => style::dim("not found in nature"),
    };
    let binding = match nuc.binding {
        Some(b) => val(&format!("{:.1} keV/A", b)),
        None => style::dim("—"),
    };
    let excess = match nuc.mass_excess {
        Some(m) => val(&format!("{:.1} keV", m)),
        None => style::dim("—"),
    };
    let found = match nuc.discovery {
        Some(y) => val(&y.to_string()),
        None => style::dim("—"),
    };
    let jp = if nuc.jp.is_empty() { style::dim("—") } else { val(&nuc.jp) };

    let col2 = 46usize.min(cols as usize / 2);
    let pair = |a: &str, b: &str| {
        let w = crust::display_width(a);
        format!("{a}{}{b}", " ".repeat(col2.saturating_sub(w)))
    };
    let lines = [
        pair(
            &format!("{} {}", key("half-life "), half),
            &format!("{} {}", key("abundance "), abundance),
        ),
        pair(
            &format!("{} {}", key("decays    "), decays),
            &format!("{} {}", key("binding   "), binding),
        ),
        pair(
            &format!("{} {}", key("spin/par  "), jp),
            &format!("{} {}", key("mass excess "), excess),
        ),
        pair(
            &format!("{} {}", key("reported  "), found),
            format!("{} {}", key("legend    "), legend(app.mode)).as_str(),
        ),
        format!("{} {}", key("chain     "), chain_line),
    ];
    for (i, line) in lines.iter().enumerate() {
        print!(
            "{} {}{}{}",
            Cursor::at(1, y0 + i as u16),
            crust::truncate_ansi(line, cols as usize - 2),
            style::RESET,
            seq::ERASE_EOL
        );
    }
    let status = match &app.status {
        Some((msg, rgb)) => style::rgb(&format!(" {msg}"), Some(*rgb), None, ""),
        None => String::new(),
    };
    print!(
        "{}{}{}{}",
        Cursor::at(1, y0 + lines.len() as u16),
        crust::truncate_ansi(&status, cols as usize),
        style::RESET,
        seq::ERASE_EOL
    );
}

/// "(1 step)" rather than "(1 steps)".
fn steps(n: usize) -> String {
    if n == 1 {
        "   (1 step)".to_string()
    } else {
        format!("   ({n} steps)")
    }
}

/// What the colours mean under the current mode.
fn legend(mode: usize) -> String {
    let chip = |rgb: (u8, u8, u8), text: &str| {
        format!("{} {}", style::rgb("█", Some(rgb), None, ""), style::dim(text))
    };
    match mode {
        1 => format!("{}  {}", style::dim("short"), gradient_bar("long")),
        2 => format!("{}  {}", style::dim("loose"), gradient_bar("bound")),
        3 => format!("{}  {}", style::dim("rare"), gradient_bar("common")),
        4 => format!("{}  {}", style::dim("1900"), gradient_bar("today")),
        _ => [
            chip(Decay::Stable.rgb(), "stable"),
            chip(Decay::BetaMinus.rgb(), "β−"),
            chip(Decay::BetaPlus.rgb(), "β+/EC"),
            chip(Decay::Alpha.rgb(), "α"),
            chip(Decay::Fission.rgb(), "SF"),
            chip(Decay::Proton.rgb(), "p"),
            chip(Decay::Neutron.rgb(), "n"),
        ]
        .join(" "),
    }
}

fn gradient_bar(tail: &str) -> String {
    let mut s = String::new();
    for i in 0..12 {
        let t = i as f64 / 11.0;
        let rgb = data::heat(t);
        s.push_str(&style::rgb("█", Some(rgb), None, ""));
    }
    format!("{s} {}", style::dim(tail))
}

fn keys_line(app: &App) -> String {
    let overview = if app.view == View::Overview { "z chart" } else { "z overview" };
    style::dim(&format!(
        "←↓↑→ move · Tab stable · ⏎ decay chain · 1-5/m colour · {overview} · / find · c claude · e csv · r redraw · ? help · q"
    ))
}

// ─────────────────────────── popups ──────────────────────────────────

fn show_chain(app: &App, cols: u16, rows: u16) {
    let t = table();
    let start = t.index(app.z, app.n).unwrap_or(0);
    let chain = t.chain(start);
    let mut text = format!(
        "{}\n\n",
        style::rgb(
            &format!("Decay chain from {}", t.all[start].name()),
            Some(ASK_RGB),
            None,
            "b"
        )
    );
    if chain.len() < 2 {
        text.push_str(&if t.all[start].is_stable() {
            "This one is stable. Nothing to follow.".to_string()
        } else {
            format!(
                "{} decays by {}, which has no single daughter to follow.\n\nFission fragments \
                 and unknown modes end a chain here.",
                t.all[start].name(),
                t.all[start].main_decay().label()
            )
        });
    } else {
        for (i, &(idx, mode)) in chain.iter().enumerate() {
            let n = &t.all[idx];
            let arrow = if i == 0 {
                "        ".to_string()
            } else {
                format!("  {:<6}", format!("{}→", mode.short()))
            };
            text.push_str(&format!(
                "{}{}  {}  {}\n",
                style::rgb(&arrow, Some(mode.rgb()), None, ""),
                style::rgb(&format!("{:<8}", n.name()), Some((235, 235, 240)), None, "b"),
                style::dim(&format!("{:>14}", n.half_life_pretty())),
                style::dim(&if n.is_stable() {
                    "the end".to_string()
                } else {
                    n.decay_summary(2)
                })
            ));
        }
        let end = &t.all[chain.last().unwrap().0];
        text.push_str(&format!(
            "\n{}\n",
            style::dim(&format!(
                "{}, ending at {}{}",
                steps(chain.len() - 1).trim().trim_matches(['(', ')']),
                end.name(),
                if end.is_stable() { ", which is stable" } else { "" }
            ))
        ));
    }
    text.push_str(&style::dim("\nESC or q closes this."));
    let w = cols.saturating_sub(8).min(78);
    let h = rows.saturating_sub(4).min(30);
    let mut p = Popup::centered(w, h, 252, 234);
    p.view(&text);
}

fn show_help(cols: u16, rows: u16) {
    let help = format!(
        "{}\n\n  \
         Neutrons run across, protons up: one cell per nuclide, {} of them.\n  \
         The valley of stability is not drawn, it falls out of the data.\n\n  \
         MOVING\n    \
           ← →, h l          along one element's isotopes, skipping the gaps\n    \
           ↑ ↓, k j          up and down the elements, keeping near the same N\n    \
           PgUp/PgDn, K J    ten elements at a time\n    \
           Tab / Shift-Tab   the next stable nuclide up or down the chart\n    \
           Home / End        lightest / heaviest isotope of this element\n    \
           g / G             hydrogen / the heaviest element\n    \
           /                 find one: U-238, fe56, 14C\n\n  \
         LOOKING\n    \
           1-5, m            colour by: decay mode · half-life · binding energy ·\n    \
                             natural abundance · year first reported\n    \
           z                 the whole chart at once, in braille\n    \
           ENTER             follow the decay chain to its stable end\n    \
           c                 ask Claude about this nuclide\n    \
           e                 write the table to ~/isotopes.csv\n    \
           ? q               this help · quit\n\n  \
         The data is the IAEA's evaluated ground-state table: half-lives,\n  \
         decay modes with their branchings, spin and parity, binding energy,\n  \
         mass excess, natural abundance and the year each was first reported.\n  \
         It ships inside the binary, so nothing here needs a network.\n\n  \
         {}",
        style::rgb(&format!("isotopes v{VERSION}"), Some(ASK_RGB), None, "b"),
        table().all.len(),
        style::dim("ESC or q closes this.")
    );
    let w = cols.saturating_sub(8).min(80);
    let h = rows.saturating_sub(4).min(32);
    let mut p = Popup::centered(w, h, 252, 234);
    p.view(&help);
}

// ─────────────────────────── the rest ────────────────────────────────

fn export(app: &App) -> Result<String, String> {
    let t = table();
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let path = format!("{home}/isotopes.csv");
    let mut out = String::from(
        "nuclide,z,n,a,half_life_s,half_life,decays,abundance_pct,binding_kev_per_a,\
         mass_excess_kev,spin_parity,discovered\n",
    );
    let mut order: Vec<usize> = (0..t.all.len()).collect();
    order.sort_by(|&a, &b| {
        let (x, y) = (&t.all[a], &t.all[b]);
        match app.mode {
            1 => y.half_life_s.unwrap_or(f64::INFINITY).total_cmp(&x.half_life_s.unwrap_or(f64::INFINITY)),
            2 => y.binding.unwrap_or(0.0).total_cmp(&x.binding.unwrap_or(0.0)),
            3 => y.abundance.unwrap_or(0.0).total_cmp(&x.abundance.unwrap_or(0.0)),
            4 => x.discovery.unwrap_or(9999).cmp(&y.discovery.unwrap_or(9999)),
            _ => (x.z, x.n).cmp(&(y.z, y.n)),
        }
    });
    for i in order {
        let n = &t.all[i];
        let decays = n
            .decays
            .iter()
            .map(|(_, c, p)| match p {
                Some(p) => format!("{c} {p}%"),
                None => c.clone(),
            })
            .collect::<Vec<_>>()
            .join(" | ");
        out.push_str(&format!(
            "{},{},{},{},{},{},\"{}\",{},{},{},\"{}\",{}\n",
            n.name(),
            n.z,
            n.n,
            n.a(),
            n.half_life_s.map(|t| t.to_string()).unwrap_or_default(),
            n.half_life_pretty(),
            decays,
            n.abundance.map(|a| a.to_string()).unwrap_or_default(),
            n.binding.map(|b| format!("{b:.2}")).unwrap_or_default(),
            n.mass_excess.map(|m| format!("{m:.2}")).unwrap_or_default(),
            n.jp,
            n.discovery.map(|y| y.to_string()).unwrap_or_default(),
        ));
    }
    std::fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(path)
}

fn claude_run(prompt: &str, input: &str) -> Result<String, String> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("claude")
        .args(["-p", prompt])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => "claude not on PATH".to_string(),
            _ => format!("spawn: {e}"),
        })?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(input.as_bytes()).map_err(|e| format!("stdin: {e}"))?;
    }
    drop(child.stdin.take());
    let out = child.wait_with_output().map_err(|e| format!("wait: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().next().unwrap_or("(no message)").chars().take(80).collect());
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn ask_claude(app: &App, question: &str) -> Result<String, String> {
    let t = table();
    let n = app.cur();
    let mut ctx = format!(
        "Nuclide: {} — {} protons, {} neutrons, mass number {}.\n\
         Half-life: {}. Decay modes: {}.\n\
         Spin/parity: {}. Binding energy: {} keV per nucleon. Mass excess: {} keV.\n",
        n.name(),
        n.z,
        n.n,
        n.a(),
        n.half_life_pretty(),
        if n.decays.is_empty() {
            "none, it is stable".to_string()
        } else {
            n.decays
                .iter()
                .map(|(_, c, p)| match p {
                    Some(p) => format!("{c} {p}%"),
                    None => c.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ")
        },
        if n.jp.is_empty() { "unknown" } else { &n.jp },
        n.binding.map(|b| format!("{b:.1}")).unwrap_or_else(|| "unknown".into()),
        n.mass_excess.map(|m| format!("{m:.1}")).unwrap_or_else(|| "unknown".into()),
    );
    if let Some(a) = n.abundance {
        ctx.push_str(&format!("Natural abundance: {a}% of this element on Earth.\n"));
    }
    if let Some(y) = n.discovery {
        ctx.push_str(&format!("First reported: {y}.\n"));
    }
    let chain = t.chain(t.index(n.z, n.n).unwrap_or(0));
    if chain.len() > 1 {
        let path: Vec<String> = chain
            .iter()
            .map(|&(i, m)| {
                if m == Decay::Stable {
                    t.all[i].name()
                } else {
                    format!("{} {}", m.short(), t.all[i].name())
                }
            })
            .collect();
        ctx.push_str(&format!("Decay chain: {}\n", path.join(" → ")));
    }
    if !app.chat.is_empty() {
        ctx.push_str("\nEarlier in this conversation:\n");
        for (q, a) in &app.chat {
            ctx.push_str(&format!("User: {q}\nYou: {a}\n\n"));
        }
    }
    ctx.push_str(&format!("\nQuestion: {question}\n"));
    claude_run(
        "You are a nuclear physicist answering inside a terminal app. Answer in plain \
         text, no markdown, under 200 words unless the question demands more. The data \
         above is from the IAEA evaluated table.",
        &ctx,
    )
}
