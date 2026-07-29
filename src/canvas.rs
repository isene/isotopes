//! The whole chart at once, in braille.
//!
//! A braille cell is a 2×4 dot matrix, so eight nuclides fit in one
//! character. All 3,386 of them land in 90×30 cells, which fits any
//! terminal worth using. Colour is per cell, so the most telling nuclide
//! in each block wins it: a stable one over a long-lived one, a
//! long-lived one over a fleeting one.

use crate::data::{rgb_for, table};
use crust::style;
use crust::Cursor;

const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// Draw the whole chart into the rectangle at (`x`, `y`).
pub fn overview(cur_z: u32, cur_n: u32, mode: usize, x: u16, y: u16, w: u16, h: u16) {
    let t = table();
    // Left margin for the proton labels.
    let label_w: u16 = 5;
    // The chart is 90 braille cells wide and 30 tall, whatever the
    // terminal is; the rest of the rectangle gets blanked so the view
    // this one replaced does not show through around it.
    let cw = ((t.n_max as usize / 2) + 1).min(w.saturating_sub(label_w).max(10) as usize);
    let rows_used = (t.z_max as usize / 4) + 1;
    let ch = rows_used.min(h.max(4) as usize);

    let mut bits = vec![0u8; cw * ch];
    let mut color = vec![None; cw * ch];
    let mut rank = vec![f64::NEG_INFINITY; cw * ch];

    for nuc in &t.all {
        // Protons up the screen, so the top row is the heaviest element.
        let row = (t.z_max - nuc.z) as usize;
        let col = nuc.n as usize;
        let (cx, cy) = (col / 2, row / 4);
        if cx >= cw || cy >= ch {
            continue;
        }
        let i = cy * cw + cx;
        bits[i] |= DOTS[row % 4][col % 2];
        // Stable nuclides own their cell; after that, the longest-lived.
        let r = if nuc.is_stable() {
            f64::INFINITY
        } else {
            nuc.half_life_s.unwrap_or(0.0).max(1e-12).log10()
        };
        if r > rank[i] {
            rank[i] = r;
            color[i] = Some(rgb_for(nuc, mode));
        }
    }

    let mut out = String::new();
    for row in 0..ch {
        out.push_str(&Cursor::at(x, y + row as u16));
        // Label every fourth braille row, which is every sixteenth
        // element: enough to find your way without a wall of numbers.
        let z_top = t.z_max as i32 - (row * 4) as i32;
        let label = if row % 4 == 0 && z_top >= 0 {
            format!("{z_top:>3}  ")
        } else {
            " ".repeat(label_w as usize)
        };
        out.push_str(&style::dim(&label));

        let mut cur: Option<(u8, u8, u8)> = None;
        for cx in 0..cw {
            let i = row * cw + cx;
            if bits[i] == 0 {
                out.push_str(style::RESET);
                cur = None;
                out.push(' ');
                continue;
            }
            // The cursor's own cell, marked rather than coloured.
            if (t.z_max - cur_z) as usize / 4 == row && cur_n as usize / 2 == cx {
                out.push_str(style::RESET);
                cur = None;
                out.push_str(&style::rgb("◆", Some((255, 255, 255)), None, "b"));
                continue;
            }
            if color[i] != cur {
                if let Some((r, g, b)) = color[i] {
                    out.push_str(&style::set_fg_rgb(r, g, b));
                    cur = color[i];
                }
            }
            out.push(char::from_u32(0x2800 + bits[i] as u32).unwrap_or(' '));
        }
        out.push_str(style::RESET);
        out.push_str(&" ".repeat((w as usize).saturating_sub(label_w as usize + cw)));
    }
    // Neutron ruler under it, one label per twenty neutrons.
    out.push_str(&Cursor::at(x, y + ch as u16));
    let mut ruler = " ".repeat(label_w as usize);
    let mut n = 0usize;
    while n <= t.n_max as usize && n / 2 < cw {
        let col = label_w as usize + n / 2;
        while ruler.len() < col {
            ruler.push(' ');
        }
        ruler.truncate(col);
        ruler.push_str(&n.to_string());
        n += 20;
    }
    out.push_str(&style::dim(&format!("{ruler:<width$}", width = w as usize)));
    // Blank whatever is left of the rectangle below the chart.
    for row in (ch + 1)..(h as usize) {
        out.push_str(&Cursor::at(x, y + row as u16));
        out.push_str(&" ".repeat(w as usize));
    }
    print!("{out}");
}
