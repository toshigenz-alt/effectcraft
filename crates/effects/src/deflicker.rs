//! Deflicker (Effect ▸ Time): removes frame-to-frame brightness flicker (exposure pumping,
//! mains-frequency lights, the brightness "breathing" of generated video) and keeps real changes:
//! cuts, dissolves, fades, moving content.
//!
//! EffectCraft's own effect: After Effects has no built-in equivalent.
//!
//! For the frame being rendered it reads the layer at up to *Frames Either Side* neighbouring
//! frames ([`EffectHost::self_at`]) and reduces each frame to a small thumbnail of the layer's
//! bounds, measured on a 4 × 4 grid of cells.
//!
//! - **Brightness.** A neighbour's brightness relative to the current frame is the median, over
//!   the cells, of the ratio of their means. So a light or an object that changes a few cells does
//!   not count as flicker.
//! - **Is it the same picture?** Flicker scales every cell by the same ratio. A content change
//!   (motion, a dissolve, a cut, another shot) changes each cell differently. The spread of the
//!   cells' log ratios around their median measures that, and a neighbour is trusted less the
//!   larger its spread, down to not at all at *Motion Tolerance*. On the clip this was tuned on
//!   (one AI-generated product shot), flicker and still frames had spreads up to 0.005 between
//!   adjacent frames and 0.015 seven frames apart, and the pour and dissolve 0.02 – 0.13 between
//!   adjacent frames. So moving or changing footage is left alone rather than damaged.
//! - **The fit.** The trusted neighbours' ratios are fitted with a robust local level weighted
//!   toward near frames (tricube over the window, then bisquare on the residuals so an odd
//!   neighbour counts less). A window with as much on either side follows a linear ramp exactly,
//!   so the inside of a fade is kept. Near where a fade baked into the footage starts or stops the
//!   level is bent by the ramp on one side (an Opacity fade on the layer is not seen: the effect
//!   reads the layer before its Transform). The current frame itself is left out of the fit, so
//!   its own flicker cannot pull the fit toward it. Ratios, not their logs, are fitted: a fade
//!   ramps the pixel values linearly. (A local line that took over where the ramp's slope was
//!   significant was tried: it measured no better on the clip, and a level is simpler.)
//! - **The correction.** The fitted ratio at the current frame is the brightness the frame should
//!   have relative to what it has. It is scaled down when fewer than half the window's weight was
//!   trusted (little evidence, little correction), clamped to × 0.5 … × 2, scaled by *Amount* in
//!   log terms and multiplied into R, G and B (one gain per channel with *Correct: Each
//!   Channel*). So the effect never moves, blurs or ghosts pixels. Flicker of texture inside the
//!   frame is not its job.
//!
//! [`EffectHost::self_at`]: crate::EffectHost::self_at

use effectcraft_raster::Image;
use rayon::prelude::*;

use crate::{Buf, EffectCtx, EffectSpec, num, p, popup, slider};
use effectcraft_keyframe::Value;

pub const ID: &str = "ec.time.deflicker";

/// Cells per side of the brightness grid.
const GRID: usize = 4;
/// Long side of the thumbnail each frame is reduced to.
const THUMB: f64 = 96.0;
/// Below this a cell's mean counts as black: its log means nothing.
const BLACK: f64 = 1e-3;
/// The largest correction, as a log gain (× 0.5 … × 2).
const MAX_LOG_GAIN: f64 = std::f64::consts::LN_2;
/// Robust refits (bisquare reweighting of the residuals).
const ROBUST_PASSES: usize = 3;

/// Channels measured per cell: R, G, B and luma.
const CH: usize = 4;
const LUMA: usize = 3;

pub fn specs() -> Vec<EffectSpec> {
    vec![EffectSpec {
        id: ID,
        name: "Deflicker",
        category: "Time",
        params: vec![
            p("framesEitherSide", "Frames Either Side", num(15.0), slider(1.0, 30.0, 1.0, 30.0, 0)),
            p("amount", "Amount", num(100.0), slider(0.0, 100.0, 0.0, 100.0, 1)),
            p("motionTolerance", "Motion Tolerance", num(3.0), slider(0.1, 50.0, 0.5, 20.0, 1)),
            p("correct", "Correct", Value::Enum(0), popup(&["Luminance", "Each Channel"])),
        ],
        render: deflicker,
        gpu: false,
        float: true,
    }]
}

fn luma(p: [f32; 4]) -> f64 {
    0.2126 * p[0] as f64 + 0.7152 * p[1] as f64 + 0.0722 * p[2] as f64
}

/// The layer's bounds in `b`, box-averaged down to at most [`THUMB`] pixels on the long side.
/// `None` when the bounds or the buffer are empty or the geometry is not finite.
fn thumbnail(b: &Buf, layer_size: [f64; 2]) -> Option<Image> {
    let [lw, lh] = layer_size;
    let s = b.scale;
    if !(lw >= 1.0 && lh >= 1.0 && s > 0.0 && lw.is_finite() && lh.is_finite() && s.is_finite()) || b.img.is_empty() {
        return None;
    }
    if !(b.offset[0].is_finite() && b.offset[1].is_finite()) {
        return None;
    }
    // The layer's extent in buffer pixels, shrunk to the thumbnail size.
    let (pw, ph) = (lw * s, lh * s);
    let k = (THUMB / pw.max(ph)).min(1.0);
    let tw = (pw * k).round().clamp(1.0, THUMB) as usize;
    let th = (ph * k).round().clamp(1.0, THUMB) as usize;
    let mut sum = vec![[0f64; 4]; tw * th];
    let mut cnt = vec![0u32; tw * th];
    let bw = b.img.width as usize;
    for (by, row) in b.img.data.chunks(bw.max(1)).enumerate() {
        let ly = (by as f64 + 0.5 - b.offset[1]) / s;
        if !(0.0..lh).contains(&ly) {
            continue;
        }
        let ty = ((ly / lh * th as f64) as usize).min(th - 1);
        for (bx, px) in row.iter().enumerate() {
            let lx = (bx as f64 + 0.5 - b.offset[0]) / s;
            if !(0.0..lw).contains(&lx) {
                continue;
            }
            let tx = ((lx / lw * tw as f64) as usize).min(tw - 1);
            let i = ty * tw + tx;
            if let (Some(acc), Some(c)) = (sum.get_mut(i), cnt.get_mut(i)) {
                for (a, v) in acc.iter_mut().zip(px) {
                    *a += *v as f64;
                }
                *c += 1;
            }
        }
    }
    let data = sum.iter().zip(&cnt).map(|(a, &c)| if c > 0 { a.map(|v| (v / c as f64) as f32) } else { [0.0; 4] }).collect();
    Some(Image { width: tw as u32, height: th as u32, data })
}

/// One frame reduced to what Deflicker compares: per cell, the mean R, G, B and luma (straight
/// colour), `None` for a cell with no opaque pixels.
struct Probe(Vec<Option<[f64; CH]>>);

/// What a neighbour says about the current frame.
struct Reading {
    /// Its brightness relative to the current frame per channel (R, G, B, luma), as a ratio; `None`
    /// for a channel no cell measures (black in either frame).
    ratio: [Option<f64>; CH],
    /// The spread of the cells' log luma ratios around their median: 0 when the neighbour is the
    /// same picture at another brightness.
    spread: f64,
}

impl Probe {
    fn of(b: &Buf, layer_size: [f64; 2]) -> Option<Probe> {
        let thumb = thumbnail(b, layer_size)?;
        let (tw, th) = (thumb.width as usize, thumb.height as usize);
        // Per cell: premultiplied R, G, B, luma and alpha sums.
        let mut acc = vec![[0f64; CH + 1]; GRID * GRID];
        for (y, row) in thumb.data.chunks(tw.max(1)).enumerate() {
            let cy = (y * GRID / th).min(GRID - 1);
            for (x, px) in row.iter().enumerate() {
                let cx = (x * GRID / tw).min(GRID - 1);
                let Some(a) = acc.get_mut(cy * GRID + cx) else { continue };
                a[0] += px[0] as f64;
                a[1] += px[1] as f64;
                a[2] += px[2] as f64;
                a[LUMA] += luma(*px);
                a[CH] += px[3] as f64;
            }
        }
        let cells: Vec<_> = acc.iter().map(|a| (a[CH] > 1e-6).then(|| [a[0] / a[CH], a[1] / a[CH], a[2] / a[CH], a[LUMA] / a[CH]])).collect();
        cells.iter().any(Option::is_some).then_some(Probe(cells))
    }

    /// The log ratios of `self`'s cells to `cur`'s in channel `ch`, over the cells both measure.
    fn log_ratios(&self, cur: &Probe, ch: usize) -> Vec<f64> {
        self.0
            .iter()
            .zip(&cur.0)
            .filter_map(|(a, b)| {
                let (a, b) = (a.as_ref()?.get(ch).copied()?, b.as_ref()?.get(ch).copied()?);
                (a > BLACK && b > BLACK).then(|| a.ln() - b.ln())
            })
            .filter(|v| v.is_finite())
            .collect()
    }

    fn reading(&self, cur: &Probe) -> Option<Reading> {
        let ratio = std::array::from_fn(|ch| median(&mut self.log_ratios(cur, ch)).map(f64::exp));
        let mut d = self.log_ratios(cur, LUMA);
        let m = median(&mut d)?;
        let mut dev: Vec<f64> = d.iter().map(|v| (v - m).abs()).collect();
        Some(Reading { ratio, spread: median(&mut dev)? })
    }
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let m = v.len() / 2;
    if v.len() % 2 == 1 { v.get(m).copied() } else { Some((v.get(m - 1)? + v.get(m)?) * 0.5) }
}

fn tricube(u: f64) -> f64 {
    let u = u.abs();
    if u >= 1.0 { 0.0 } else { (1.0 - u * u * u).powi(3) }
}

fn bisquare(u: f64) -> f64 {
    let u = u.abs();
    if u >= 1.0 { 0.0 } else { (1.0 - u * u).powi(2) }
}

/// One neighbour in the fit: its value and weight (distance × trust).
#[derive(Clone, Copy)]
struct Pt {
    y: f64,
    w: f64,
}

/// Weighted mean of the values; `None` without weight.
fn wmean(pts: &[Pt], w: &[f64]) -> Option<f64> {
    let (s, n) = pts.iter().zip(w).fold((0.0, 0.0), |(s, n), (p, &w)| (s + w * p.y, n + w));
    (n > 1e-12).then(|| s / n)
}

/// A robust weighted level: the weighted mean, refitted with bisquare weights on the residuals so
/// an odd neighbour (a flicker peak, a flash) counts less.
fn robust_level(pts: &[Pt]) -> Option<f64> {
    let base: Vec<f64> = pts.iter().map(|p| p.w).collect();
    let mut a = wmean(pts, &base)?;
    for _ in 0..ROBUST_PASSES {
        let res: Vec<f64> = pts.iter().map(|p| (p.y - a).abs()).collect();
        let mut live: Vec<f64> = res.iter().zip(&base).filter(|(_, w)| **w > 0.0).map(|(r, _)| *r).collect();
        let Some(s) = median(&mut live) else { break };
        if s.is_nan() || s <= 1e-9 {
            break;
        }
        let w: Vec<f64> = base.iter().zip(&res).map(|(b, r)| b * bisquare(r / (6.0 * s))).collect();
        match wmean(pts, &w) {
            Some(n) => a = n,
            None => break,
        }
    }
    a.is_finite().then_some(a)
}

/// The brightness ratio the current frame should have (see the module docs), from `pts`.
fn target(pts: &[Pt]) -> Option<f64> {
    robust_level(pts)
}

fn deflicker(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let amount = ctx.params.f("amount") / 100.0;
    let Some(host) = ctx.env.host else { return b };
    if amount.is_nan() || amount <= 0.0 {
        return b;
    }
    let amount = amount.min(1.0);
    let fps = ctx.fps();
    let n = ctx.params.f("framesEitherSide");
    let n = if n.is_finite() { n.round().clamp(1.0, 30.0) as i64 } else { 15 };
    let tol = ctx.params.f("motionTolerance") / 100.0;
    let tol = if tol.is_finite() { tol.clamp(0.001, 0.5) } else { 0.03 };
    let each = ctx.params.e("correct") == 1;
    let Some(cur) = Probe::of(&b, ctx.layer_size) else { return b };

    // Per channel: the trusted neighbours.
    let mut pts: [Vec<Pt>; CH] = Default::default();
    let mut trusted = 0.0;
    for dir in [-1i64, 1] {
        for k in 1..=n {
            let kf = (dir * k) as f64;
            let Some(nb) = host.self_at(ctx.time + kf / fps, usize::MAX) else { break };
            let Some(r) = Probe::of(&nb, ctx.layer_size).and_then(|p| p.reading(&cur)) else { break };
            let w = tricube(kf / (n + 1) as f64) * bisquare(r.spread / tol);
            trusted += w;
            for (ch, ratio) in r.ratio.into_iter().enumerate() {
                if let (Some(y), Some(list)) = (ratio, pts.get_mut(ch)) {
                    list.push(Pt { y, w });
                }
            }
        }
    }
    // Little trusted evidence, little correction: full strength from half a window's weight.
    let full: f64 = (1..=n).map(|k| 2.0 * tricube(k as f64 / (n + 1) as f64)).sum();
    let confidence = if full > 0.0 { (trusted / (0.5 * full)).min(1.0) } else { 0.0 };
    if confidence.is_nan() || confidence <= 0.0 {
        return b;
    }
    let gain = |ch: usize| -> f32 {
        let fit = pts.get(ch).and_then(|p| target(p)).filter(|r| *r > 0.0).map_or(0.0, f64::ln);
        (fit.clamp(-MAX_LOG_GAIN, MAX_LOG_GAIN) * amount * confidence).exp() as f32
    };
    let g = if each { [gain(0), gain(1), gain(2)] } else { [gain(LUMA); 3] };
    if g.iter().all(|&v| v == 1.0) {
        return b;
    }
    b.img.data.par_iter_mut().for_each(|px| {
        px[0] *= g[0];
        px[1] *= g[1];
        px[2] *= g[2];
    });
    b
}

#[cfg(test)]
mod tests;
