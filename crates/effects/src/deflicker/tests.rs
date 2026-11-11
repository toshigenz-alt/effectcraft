use super::*;
use crate::{EffectEnv, EffectHost, LayerPixels, run_fx};

const FPS: f64 = 10.0;
const W: u32 = 64;
const H: u32 = 40;

/// A textured frame: `shot` picks the picture, `dx` pans it, `gain` scales R, G, B (each).
fn frame(shot: u32, dx: f32, gain: [f32; 3]) -> Image {
    let mut img = Image::new(W, H);
    for y in 0..H {
        for x in 0..W {
            let fx = x as f32 + dx;
            let c = match shot {
                0 => {
                    let v = 0.45 + 0.3 * (fx * 0.19).sin() * (y as f32 * 0.23).cos();
                    [v, v * 0.8, 0.25 + 0.2 * (y as f32 * 0.1).sin()]
                }
                1 => {
                    let v = if ((fx / 8.0).floor() as i32 + (y / 8) as i32) % 2 == 0 { 0.7 } else { 0.1 };
                    [0.1, v, v * 0.6 + 0.2]
                }
                // A different random block picture per `shot`: content that changes every frame.
                s => {
                    let v = 0.15 + 0.7 * hash01(s, (fx / 10.0).floor() as i32, (y / 10) as i32);
                    [v, v, v * 0.9]
                }
            };
            img.set(x, y, [c[0] * gain[0], c[1] * gain[1], c[2] * gain[2], 1.0]);
        }
    }
    img
}

fn hash01(a: u32, b: i32, c: i32) -> f32 {
    let h = a.wrapping_mul(0x9e37_79b9) ^ (b as u32).wrapping_mul(0x85eb_ca6b) ^ (c as u32).wrapping_mul(0xc2b2_ae35);
    let h = (h ^ (h >> 15)).wrapping_mul(0x2c1b_3c6d);
    ((h ^ (h >> 12)) >> 8) as f32 / (1u32 << 24) as f32
}

/// A clip: frame `i` is at layer time `i / FPS`; outside the clip there is no frame.
struct Clip(Vec<Image>);

impl EffectHost for Clip {
    fn layer(&self, _: u64, _: bool) -> Option<LayerPixels> {
        None
    }
    fn audio(&self, _: u64, _: f64, _: usize, _: u32) -> Option<Vec<f32>> {
        None
    }
    fn self_at(&self, t: f64, _: usize) -> Option<Buf> {
        let i = (t * FPS).round();
        let img = if i < 0.0 { None } else { self.0.get(i as usize) }?;
        Some(Buf { img: img.clone(), offset: [0.0; 2], scale: 1.0 })
    }
}

fn run(clip: &Clip, i: usize, vals: &[(&str, Value)]) -> Image {
    let t = i as f64 / FPS;
    let env = EffectEnv { host: Some(clip), frame_rate: FPS, comp_time: t, ..Default::default() };
    run_fx(ID, vals, clip.0[i].clone(), t, env).img
}

fn mean(img: &Image, ch: usize) -> f64 {
    img.data.iter().map(|p| p[ch] as f64).sum::<f64>() / img.data.len() as f64
}

fn mean_luma(img: &Image) -> f64 {
    img.data.iter().map(|p| luma(*p)).sum::<f64>() / img.data.len() as f64
}

/// |a / b - 1| of the mean luma.
fn err(a: &Image, b: &Image) -> f64 {
    (mean_luma(a) / mean_luma(b) - 1.0).abs()
}

/// RMS over `frames` of the log mean-luma error of `out(i)` against `want[i]`.
fn rms(frames: std::ops::RangeInclusive<usize>, out: &dyn Fn(usize) -> Image, want: &[Image]) -> f64 {
    let e: Vec<f64> = frames.map(|i| (mean_luma(&out(i)) / mean_luma(&want[i])).ln()).collect();
    (e.iter().map(|v| v * v).sum::<f64>() / e.len() as f64).sqrt()
}

fn scaled(img: &Image, g: f32) -> Image {
    let mut out = img.clone();
    for p in &mut out.data {
        for v in p.iter_mut().take(3) {
            *v *= g;
        }
    }
    out
}

/// Deterministic flicker in -1 … 1 per frame.
fn wobble(i: usize) -> f32 {
    hash01(7, i as i32, 3) * 2.0 - 1.0
}

#[test]
fn removes_random_flicker_from_a_still() {
    let clean: Vec<Image> = (0..41).map(|_| frame(0, 0.0, [1.0; 3])).collect();
    let clip = Clip(clean.iter().enumerate().map(|(i, f)| scaled(f, 1.0 + 0.15 * wobble(i))).collect());
    let before = rms(12..=28, &|i| clip.0[i].clone(), &clean);
    let after = rms(12..=28, &|i| run(&clip, i, &[]), &clean);
    eprintln!("still: random flicker RMS {before:.4} -> {after:.4}");
    // The fixture can fail: the input is far off.
    assert!(before > 0.06, "input flicker {before}");
    assert!(after < 0.3 * before, "left after Deflicker {after} (input {before})");
}

/// Known limit: a pan changes the cells, so far neighbours are trusted less and less flicker is
/// removed. 0.3 px per frame on this 64 px picture is about 6 px per frame on a 1280 px frame.
/// Measured when written: RMS 0.086 -> 0.053.
#[test]
fn a_slow_pan_is_partly_corrected() {
    let clean: Vec<Image> = (0..41).map(|i| frame(0, i as f32 * 0.3, [1.0; 3])).collect();
    let clip = Clip(clean.iter().enumerate().map(|(i, f)| scaled(f, 1.0 + 0.15 * wobble(i))).collect());
    let before = rms(12..=28, &|i| clip.0[i].clone(), &clean);
    let after = rms(12..=28, &|i| run(&clip, i, &[]), &clean);
    eprintln!("slow pan: random flicker RMS {before:.4} -> {after:.4}");
    assert!(after < 0.7 * before, "left after Deflicker {after} (input {before})");
}

#[test]
fn strong_alternating_flicker_is_corrected() {
    // ±30 % every other frame.
    let clean: Vec<Image> = (0..41).map(|_| frame(0, 0.0, [1.0; 3])).collect();
    let clip = Clip((0..41).map(|i| scaled(&clean[i], if i % 2 == 0 { 0.7 } else { 1.3 })).collect());
    let before = rms(15..=25, &|i| clip.0[i].clone(), &clean);
    let after = rms(15..=25, &|i| run(&clip, i, &[]), &clean);
    eprintln!("alternating flicker RMS {before:.4} -> {after:.4}");
    assert!(after < 0.25 * before, "left {after} of {before}");
}

#[test]
fn amount_scales_the_correction() {
    let still = frame(0, 0.0, [1.0; 3]);
    let clip = Clip((0..41).map(|i| scaled(&still, if i == 20 { 1.2 } else { 1.0 })).collect());
    let full = mean_luma(&run(&clip, 20, &[]));
    let half = mean_luma(&run(&clip, 20, &[("amount", num(50.0))]));
    let none = run(&clip, 20, &[("amount", num(0.0))]);
    let clean = mean_luma(&still);
    assert!((full / clean - 1.0).abs() < 0.002, "{full} vs {clean}");
    // Half the log gain: 1.2^0.5.
    assert!((half / clean - 1.2f64.sqrt()).abs() < 0.005, "{half}");
    assert_eq!(none.data, clip.0[20].data);
}

/// A fade baked into the footage scales every cell alike, as flicker does. Inside the ramp the
/// level of a window with as much on either side follows the slope exactly. Known limit: around where
/// the ramp starts (or stops) the fit is a level bent by the ramp on one side, so those frames
/// move. Measured when written, with 15 frames either side: up to 8.2 % (7: 4.7 %). (An Opacity fade on
/// the layer is not affected: Deflicker sees the layer before its Transform.)
#[test]
fn follows_a_fade_and_bends_its_start() {
    // Brightness 1.0 for frames 0…14, then +4 % per frame.
    let still = frame(0, 0.0, [1.0; 3]);
    let clip = Clip((0..51).map(|i| scaled(&still, 1.0 + 0.04 * (i as f32 - 14.0).max(0.0))).collect());
    let worst = |r: std::ops::RangeInclusive<usize>, vals: &[(&str, Value)]| r.map(|i| err(&run(&clip, i, vals), &clip.0[i])).fold(0.0, f64::max);
    let mid = worst(30..=36, &[]);
    let corner = worst(8..=22, &[]);
    let corner7 = worst(8..=22, &[("framesEitherSide", num(7.0))]);
    eprintln!("fade: inside {mid:.4}, around the start {corner:.4} (7 frames either side: {corner7:.4})");
    assert!(mid < 0.002, "inside the ramp: {mid}");
    assert!(corner < 0.12, "around the ramp's start: {corner}");
    // A shorter window bends less.
    assert!(corner7 < corner, "{corner7} vs {corner}");
}

/// Shot 0 (bright) for frames 0…19, shot 1 (a different, darker picture) from frame 20.
fn cut_clip() -> Vec<Image> {
    (0..40).map(|i| if i < 20 { frame(0, 0.0, [1.0; 3]) } else { frame(1, 0.0, [0.6; 3]) }).collect()
}

#[test]
fn brightness_does_not_cross_a_cut() {
    let clip = Clip(cut_clip());
    for i in 16..=23 {
        let e = err(&run(&clip, i, &[]), &clip.0[i]);
        assert!(e < 0.002, "frame {i} changed {e} next to the cut");
    }
}

#[test]
fn flicker_beside_a_cut_is_corrected_from_its_own_shot() {
    let clean = cut_clip();
    let clip = Clip(clean.iter().enumerate().map(|(i, f)| scaled(f, 1.0 + 0.08 * wobble(i + 100))).collect());
    let before = rms(16..=23, &|i| clip.0[i].clone(), &clean);
    let after = rms(16..=23, &|i| run(&clip, i, &[]), &clean);
    eprintln!("flicker beside a cut RMS {before:.4} -> {after:.4}");
    assert!(after < 0.5 * before, "next to the cut: left {after} of {before}");
}

#[test]
fn changing_content_is_left_alone() {
    // A new picture every frame (a pour, a whip pan, an edit) at varying brightness: nothing here
    // says the brightness changes are flicker, so nothing may change.
    let clip = Clip((0..31).map(|i| frame(10 + i as u32, 0.0, [0.8 + 0.4 * hash01(3, i, 0); 3])).collect());
    for i in [10, 15, 20] {
        let e = err(&run(&clip, i, &[]), &clip.0[i]);
        assert!(e < 0.002, "frame {i} changed {e}");
    }
}

/// Share of the possible correction applied to frame 20 of a slow pan whose frame 20 is 25 %
/// too bright.
fn pan_correction(tolerance: f64) -> f64 {
    let clean: Vec<Image> = (0..41).map(|i| frame(0, i as f32 * 0.3, [1.0; 3])).collect();
    let clip = Clip(clean.iter().enumerate().map(|(i, f)| scaled(f, if i == 20 { 1.25 } else { 1.0 })).collect());
    let out = run(&clip, 20, &[("motionTolerance", num(tolerance))]);
    (mean_luma(&out) / mean_luma(&clip.0[20])).ln() / (mean_luma(&clean[20]) / mean_luma(&clip.0[20])).ln()
}

/// Little trusted evidence, little correction. Without it, on the clip this was tuned on, the few
/// half-trusted neighbours in a pour pulled frames toward their own brightness (its frames 1-27:
/// RMS 5.40 -> 6.49, max 20.6 -> 23.6 levels). Measured when written: at Motion Tolerance 2 a
/// third of the window's weight is trusted and 17 % of the correction is applied; 100 % without
/// the scaling.
#[test]
fn thin_evidence_gives_a_smaller_correction() {
    let share = pan_correction(2.0);
    assert!(share > 0.05 && share < 0.5, "share {share}");
    assert!(pan_correction(20.0) > 0.99);
}

#[test]
fn motion_tolerance_decides_what_counts_as_the_same_picture() {
    // A slow pan with frame 20 25 % brighter: at a low tolerance the panned neighbours are not
    // trusted and little is corrected, at a high one they all are.
    let clean: Vec<Image> = (0..41).map(|i| frame(0, i as f32 * 0.3, [1.0; 3])).collect();
    let clip = Clip(clean.iter().enumerate().map(|(i, f)| scaled(f, if i == 20 { 1.25 } else { 1.0 })).collect());
    let low = err(&run(&clip, 20, &[("motionTolerance", num(0.5))]), &clean[20]);
    let high = err(&run(&clip, 20, &[("motionTolerance", num(20.0))]), &clean[20]);
    eprintln!("motion tolerance: 0.5 leaves {low:.4}, 20 leaves {high:.4} (input 0.25)");
    assert!(low > 0.15, "low tolerance corrected to {low}");
    assert!(high < 0.03, "high tolerance left {high}");
}

#[test]
fn each_channel_fixes_a_colour_flicker() {
    // Only red flickers, on frame 20.
    let clean = frame(0, 0.0, [1.0; 3]);
    let clip = Clip((0..41).map(|i| frame(0, 0.0, [if i == 20 { 1.2 } else { 1.0 }, 1.0, 1.0])).collect());
    let out = run(&clip, 20, &[("correct", Value::Enum(1))]);
    for (ch, tol) in [(0, 0.005), (1, 0.002), (2, 0.002)] {
        let r = mean(&out, ch) / mean(&clean, ch);
        assert!((r - 1.0).abs() < tol, "channel {ch}: {r}");
    }
    // Luminance applies one gain to all three: green moves although it never flickered.
    let lum = run(&clip, 20, &[]);
    assert!((mean(&lum, 1) / mean(&clean, 1) - 1.0).abs() > 0.01);
}

#[test]
fn a_local_flash_is_not_flicker() {
    // A steady picture; on frame 20 alone a bright light fills one corner (a muzzle flash, a
    // lamp). The rest of the frame did not change, so it must not be darkened to compensate.
    let base = frame(0, 0.0, [1.0; 3]);
    let mut flash = base.clone();
    for y in 0..H / 4 {
        for x in 0..W / 4 {
            flash.set(x, y, [3.0, 3.0, 3.0, 1.0]);
        }
    }
    let clip = Clip((0..41).map(|i| if i == 20 { flash.clone() } else { base.clone() }).collect());
    let out = run(&clip, 20, &[]);
    let outside =
        |img: &Image| img.data.iter().enumerate().filter(|(i, _)| (*i as u32 % W) >= W / 4 || (*i as u32 / W) >= H / 4).map(|(_, p)| luma(*p)).sum::<f64>();
    let r = outside(&out) / outside(&clip.0[20]);
    assert!((r - 1.0).abs() < 0.002, "the unlit part changed by {r}");
}

#[test]
fn passes_through_without_a_host_or_pixels() {
    let img = frame(0, 0.0, [1.0; 3]);
    let out = run_fx(ID, &[], img.clone(), 0.0, EffectEnv::default());
    assert_eq!(out.img.data, img.data);
    // Transparent frames and odd parameters: no change, no panic.
    let clip = Clip((0..5).map(|_| Image::new(W, H)).collect());
    assert_eq!(run(&clip, 2, &[]).data, clip.0[2].data);
    let clip = Clip((0..5).map(|i| frame(0, 0.0, [1.0 + 0.1 * i as f32; 3])).collect());
    for v in [f64::NAN, f64::INFINITY, -5.0, 1e9] {
        let _ = run(&clip, 2, &[("framesEitherSide", num(v)), ("amount", num(v)), ("motionTolerance", num(v))]);
    }
    // A single frame with no neighbours.
    let one = Clip(vec![frame(0, 0.0, [1.3; 3])]);
    assert_eq!(run(&one, 0, &[]).data, one.0[0].data);
}

#[test]
fn thumbnail_reads_the_layer_bounds_at_any_scale_and_offset() {
    // The same layer at half resolution, padded by 5 px: the same cells.
    let img = frame(0, 0.0, [1.0; 3]);
    let full = Probe::of(&Buf { img: img.clone(), offset: [0.0; 2], scale: 1.0 }, [W as f64, H as f64]).unwrap();
    let half = effectcraft_raster::resample(&img, W / 2, H / 2);
    let mut pad = Image::new(W / 2 + 10, H / 2 + 10);
    for y in 0..H / 2 {
        for x in 0..W / 2 {
            pad.set(x + 5, y + 5, half.data[half.idx(x, y)]);
        }
    }
    let small = Probe::of(&Buf { img: pad, offset: [5.0; 2], scale: 0.5 }, [W as f64, H as f64]).unwrap();
    let r = small.reading(&full).unwrap();
    assert!((r.ratio[LUMA].unwrap() - 1.0).abs() < 0.02, "{:?}", r.ratio);
    assert!(r.spread < 0.02, "spread {}", r.spread);
}
