//! Rendu du personnage (reproduction fidèle des deux SVG fournis) dans un
//! buffer ARGB, avec anti-crénelage. viewBox d'origine : 280 x 460.

const VB_W: f32 = 280.0;
const VB_H: f32 = 460.0;

const BLACK: [f32; 3] = [0.0, 0.0, 0.0];
const WHITE: [f32; 3] = [1.0, 1.0, 1.0];

struct RRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
    color: [f32; 3],
}

impl RRect {
    /// Distance signée au bord du rectangle arrondi (négative à l'intérieur).
    fn sdf(&self, px: f32, py: f32) -> f32 {
        let hw = self.w / 2.0;
        let hh = self.h / 2.0;
        let r = self.r.min(hw).min(hh);
        let qx = (px - (self.x + hw)).abs() - (hw - r);
        let qy = (py - (self.y + hh)).abs() - (hh - r);
        let ox = qx.max(0.0);
        let oy = qy.max(0.0);
        (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
    }
}

fn shapes(mouth_open: bool) -> Vec<RRect> {
    let rr = |x, y, w, h, r, color| RRect { x, y, w, h, r, color };
    // <rect x=15 y=15 w=250 h=420 rx=18 fill=white stroke=black stroke-width=10/>
    // => contour noir (bord extérieur) puis remplissage blanc (bord intérieur).
    let mut v = vec![
        rr(10.0, 10.0, 260.0, 430.0, 23.0, BLACK),
        rr(20.0, 20.0, 240.0, 410.0, 13.0, WHITE),
        // yeux
        rr(85.0, 125.0, 22.0, 90.0, 11.0, BLACK),
        rr(173.0, 125.0, 22.0, 90.0, 11.0, BLACK),
    ];
    if mouth_open {
        v.push(rr(125.0, 325.0, 30.0, 40.0, 0.0, BLACK));
    } else {
        v.push(rr(125.0, 340.0, 30.0, 6.0, 3.0, BLACK));
    }
    v
}

/// Remplit `out` (w*h pixels, format Android ARGB_8888 non prémultiplié).
pub fn render(out: &mut [i32], w: usize, h: usize, mouth_open: bool) {
    if w == 0 || h == 0 || out.len() < w * h {
        return;
    }
    let scale = (w as f32 / VB_W).min(h as f32 / VB_H);
    let off_x = (w as f32 - VB_W * scale) / 2.0;
    let off_y = (h as f32 - VB_H * scale) / 2.0;
    let shapes = shapes(mouth_open);

    for y in 0..h {
        for x in 0..w {
            // coordonnées du centre du pixel dans la viewBox
            let vx = (x as f32 + 0.5 - off_x) / scale;
            let vy = (y as f32 + 0.5 - off_y) / scale;
            // composition "over" en prémultiplié
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for s in &shapes {
                let d = s.sdf(vx, vy) * scale; // en pixels
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov <= 0.0 {
                    continue;
                }
                r = s.color[0] * cov + r * (1.0 - cov);
                g = s.color[1] * cov + g * (1.0 - cov);
                b = s.color[2] * cov + b * (1.0 - cov);
                a = cov + a * (1.0 - cov);
            }
            let px = if a <= 0.0 {
                0
            } else {
                let to8 = |c: f32| ((c / a).clamp(0.0, 1.0) * 255.0).round() as u32;
                let a8 = (a.clamp(0.0, 1.0) * 255.0).round() as u32;
                ((a8 << 24) | (to8(r) << 16) | (to8(g) << 8) | to8(b)) as i32
            };
            out[y * w + x] = px;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_expected_pixels() {
        let (w, h) = (280, 460);
        let mut buf = vec![0i32; w * h];
        render(&mut buf, w, h, false);
        let px = |x: usize, y: usize| buf[y * w + x] as u32;
        assert_eq!(px(2, 2) >> 24, 0); // transparent hors du personnage
        assert_eq!(px(140, 100), 0xFFFF_FFFF); // corps blanc
        assert_eq!(px(96, 170), 0xFF00_0000); // oeil
        assert_eq!(px(140, 343), 0xFF00_0000); // bouche fermée
        assert_eq!(px(140, 330), 0xFFFF_FFFF);
        render(&mut buf, w, h, true);
        assert_eq!(buf[330 * w + 140] as u32, 0xFF00_0000); // bouche ouverte
    }
}
