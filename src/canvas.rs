//! The whole chart at once: real pixels where the terminal shows images,
//! braille elsewhere.
//!
//! A braille cell is a 2×4 dot matrix, so eight nuclides fit in one
//! character. All 3,386 of them land in 90×30 cells, which fits any
//! terminal worth using. In braille the colour is per cell, so the most
//! telling nuclide in each block wins it: a stable one over a long-lived
//! one, a long-lived one over a fleeting one. In pixels every nuclide is
//! its own square in its own colour, on the same grid.

use crate::data::{rgb_for, table};
use crust::style;
use crust::Cursor;

const DOTS: [[u8; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// The chart as a picture for `cw` × `ch` cells: a square per nuclide,
/// two across and four down each cell, the cursor's framed in white.
/// See-through between the squares.
pub fn picture(cur_z: u32, cur_n: u32, mode: usize, cw: usize, ch: usize, cell: Option<(u16, u16)>) -> glow::Canvas {
    let t = table();
    let mut c = glow::Canvas::sized(cw as u16, ch as u16, cell);
    c.see_through();
    let (ux, uy) = (c.cell_w() / 2.0, c.cell_h() / 4.0);
    let gap = (ux.min(uy) * 0.2).clamp(0.5, 2.0);
    for nuc in &t.all {
        let row = (t.z_max - nuc.z) as usize;
        let col = nuc.n as usize;
        if col / 2 >= cw || row / 4 >= ch {
            continue;
        }
        let (x0, y0) = (col as f64 * ux + gap / 2.0, row as f64 * uy + gap / 2.0);
        square(&mut c, x0, y0, x0 + ux - gap, y0 + uy - gap, rgb_for(nuc, mode), 1.0);
    }
    // The cursor: a white frame round its square.
    let (row, col) = ((t.z_max - cur_z) as f64, cur_n as f64);
    let (x0, y0, x1, y1) = (col * ux - 1.5, row * uy - 1.5, (col + 1.0) * ux + 1.5, (row + 1.0) * uy + 1.5);
    let white = (255, 255, 255);
    square(&mut c, x0, y0, x1, y0 + 1.5, white, 1.0);
    square(&mut c, x0, y1 - 1.5, x1, y1, white, 1.0);
    square(&mut c, x0, y0, x0 + 1.5, y1, white, 1.0);
    square(&mut c, x1 - 1.5, y0, x1, y1, white, 1.0);
    c
}

/// A rectangle with soft edges: each pixel gets the share of it the
/// rectangle covers.
fn square(c: &mut glow::Canvas, x0: f64, y0: f64, x1: f64, y1: f64, rgb: (u8, u8, u8), a: f64) {
    for py in y0.floor() as i64..y1.ceil() as i64 {
        let cy = (y1.min(py as f64 + 1.0) - y0.max(py as f64)).clamp(0.0, 1.0);
        for px in x0.floor() as i64..x1.ceil() as i64 {
            let cx = (x1.min(px as f64 + 1.0) - x0.max(px as f64)).clamp(0.0, 1.0);
            if cx * cy > 0.0 {
                c.blend(px, py, rgb, a * cx * cy);
            }
        }
    }
}

/// Draw the whole chart into the rectangle at (`x`, `y`). Through
/// `display` it is a picture where the terminal shows images.
pub fn overview(cur_z: u32, cur_n: u32, mode: usize, x: u16, y: u16, w: u16, h: u16, display: &mut Option<glow::Display>) {
    let t = table();
    let pixels = display.get_or_insert_with(glow::Display::new).supported();
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
        if pixels {
            // The picture covers the chart; the label column stays text.
            let z_top = t.z_max as i32 - (row * 4) as i32;
            let label = if row % 4 == 0 && z_top >= 0 { format!("{z_top:>3}  ") } else { " ".repeat(label_w as usize) };
            out.push_str(&style::dim(&label));
            out.push_str(&" ".repeat((w as usize).saturating_sub(label_w as usize)));
            continue;
        }
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
    if pixels {
        use std::io::Write;
        std::io::stdout().flush().ok();
        let canvas = picture(cur_z, cur_n, mode, cw, ch, None);
        if let Some(d) = display.as_mut() {
            d.clear_all();
            d.show_canvas(&canvas, x + label_w, y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_picture_has_a_square_per_nuclide_and_a_frame_on_the_cursor() {
        let c = picture(26, 30, 0, 90, 30, Some((10, 20)));
        assert_eq!((c.w, c.h), (900, 600));
        let solid = c.rgba.chunks(4).filter(|p| p[3] == 255).count();
        assert!(solid > 3386 * 8, "only {solid} solid pixels");
        assert_eq!(c.rgba[3], 0, "the corner is see-through");
        // Iron-56 sits at row z_max-26, column 30: its frame is white.
        let t = table();
        let (x, y) = ((30.0 * 5.0 - 1.0) as usize, (((t.z_max - 26) as f64) * 5.0 - 1.0) as usize);
        let o = (y * c.w + x) * 4;
        assert!(c.rgba[o] > 200 && c.rgba[o + 1] > 200 && c.rgba[o + 2] > 200, "frame pixel {:?}", &c.rgba[o..o + 4]);
    }
}
