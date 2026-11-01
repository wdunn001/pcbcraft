//! Measurement CLI for the datasheet renderer spike (#723).
//!
//! Context: `fixtures <dir>` writes the generated CI fixtures; `bench` renders chosen pages of one
//! PDF with hayro, extracts the text layer, and compares both with a reference rasteriser's PPM and
//! text file (from poppler, produced by `scripts/run-corpus.py`). One JSON object per line goes to
//! stdout; a final `doc` line carries open time and peak resident memory.
//!
//! Rules: a panic inside hayro is caught per page and recorded as a failure, never aborts the run.
//!
//! Do not: run this against files you did not download for the spike.

use datasheet_spike::{fixtures, open, render_page, search, text_layer};
use hayro::RenderCache;
use hayro::hayro_interpret::InterpreterCache;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

fn peak_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
        })
        .unwrap_or(0)
}

fn read_ppm(path: &str) -> Option<(usize, usize, Vec<u8>)> {
    let d = std::fs::read(path).ok()?;
    // P6\nW H\n255\n
    let mut pos = 0;
    let mut tok = Vec::new();
    while tok.len() < 4 {
        while pos < d.len() && d[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let s = pos;
        while pos < d.len() && !d[pos].is_ascii_whitespace() {
            pos += 1;
        }
        tok.push(String::from_utf8_lossy(&d[s..pos]).to_string());
    }
    pos += 1;
    let w: usize = tok[1].parse().ok()?;
    let h: usize = tok[2].parse().ok()?;
    (d.len() >= pos + w * h * 3).then(|| (w, h, d[pos..pos + w * h * 3].to_vec()))
}

fn gray_rgb(rgb: &[u8], i: usize) -> f32 {
    0.299 * f32::from(rgb[i * 3])
        + 0.587 * f32::from(rgb[i * 3 + 1])
        + 0.114 * f32::from(rgb[i * 3 + 2])
}

/// Returns (bad_block_fraction, mean_abs_diff, ink_ours, ink_ref, size_mismatch).
fn compare(
    ours: &[u8],
    ow: usize,
    oh: usize,
    rw: usize,
    rh: usize,
    rr: &[u8],
) -> (f32, f32, f32, f32, bool) {
    let w = ow.min(rw);
    let h = oh.min(rh);
    let mismatch = ow.abs_diff(rw) > 3 || oh.abs_diff(rh) > 3;
    let g_o = |x: usize, y: usize| {
        let i = (y * ow + x) * 4;
        0.299 * f32::from(ours[i]) + 0.587 * f32::from(ours[i + 1]) + 0.114 * f32::from(ours[i + 2])
    };
    let g_r = |x: usize, y: usize| gray_rgb(rr, y * rw + x);
    let b = 8;
    let (mut bad, mut tot) = (0usize, 0usize);
    let (mut sum_abs, mut n) = (0.0f64, 0usize);
    let (mut ink_o, mut ink_r) = (0usize, 0usize);
    let mut by = 0;
    while by + b <= h {
        let mut bx = 0;
        while bx + b <= w {
            let (mut so, mut sr) = (0.0f32, 0.0f32);
            for y in by..by + b {
                for x in bx..bx + b {
                    let (a, c) = (g_o(x, y), g_r(x, y));
                    so += a;
                    sr += c;
                    sum_abs += f64::from((a - c).abs());
                    n += 1;
                    if a < 128.0 {
                        ink_o += 1;
                    }
                    if c < 128.0 {
                        ink_r += 1;
                    }
                }
            }
            let d = ((so - sr) / (b * b) as f32).abs();
            if d > 40.0 {
                bad += 1;
            }
            tot += 1;
            bx += b;
        }
        by += b;
    }
    let area = (n.max(1)) as f32;
    (
        bad as f32 / tot.max(1) as f32,
        (sum_abs / n.max(1) as f64) as f32,
        ink_o as f32 / area,
        ink_r as f32 / area,
        mismatch,
    )
}

/// Fraction of 4x4 blocks whose mean grey differs by more than 25/255: sensitive to shifted or
/// substituted glyphs that the 8x8 metric averages away.
fn fine_bad(ours: &[u8], ow: usize, oh: usize, rw: usize, rh: usize, rr: &[u8]) -> f32 {
    let (w, h) = (ow.min(rw), oh.min(rh));
    let (mut bad, mut tot) = (0usize, 0usize);
    let mut by = 0;
    while by + 4 <= h {
        let mut bx = 0;
        while bx + 4 <= w {
            let (mut so, mut sr) = (0.0f32, 0.0f32);
            for y in by..by + 4 {
                for x in bx..bx + 4 {
                    let i = (y * ow + x) * 4;
                    so += 0.299 * f32::from(ours[i])
                        + 0.587 * f32::from(ours[i + 1])
                        + 0.114 * f32::from(ours[i + 2]);
                    sr += gray_rgb(rr, y * rw + x);
                }
            }
            if ((so - sr) / 16.0).abs() > 25.0 {
                bad += 1;
            }
            tot += 1;
            bx += 4;
        }
        by += 4;
    }
    bad as f32 / tot.max(1) as f32
}

fn words(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 3)
        .map(|w| w.to_lowercase())
        .collect();
    v.sort();
    v.dedup();
    v
}

fn esc(s: &str) -> String {
    s.chars()
        .flat_map(|c| {
            if c == '"' || c == '\\' {
                vec!['\\', c]
            } else if c.is_control() {
                vec![' ']
            } else {
                vec![c]
            }
        })
        .collect()
}

fn bench(args: &[String]) -> Result<(), String> {
    let path = &args[0];
    let mut pages: Vec<usize> = Vec::new();
    let (mut refdir, mut tag, mut dpi, mut png) =
        (String::new(), String::from("doc"), 150.0f32, String::new());
    let mut i = 1;
    while i + 1 < args.len() {
        match args[i].as_str() {
            "--pages" => {
                pages = args[i + 1]
                    .split(',')
                    .filter_map(|p| p.parse().ok())
                    .collect()
            }
            "--ref" => refdir = args[i + 1].clone(),
            "--tag" => tag = args[i + 1].clone(),
            "--dpi" => dpi = args[i + 1].parse().unwrap_or(150.0),
            "--png" => png = args[i + 1].clone(),
            _ => {}
        }
        i += 2;
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let t0 = Instant::now();
    let pdf = match open(bytes) {
        Ok(p) => p,
        Err(e) => {
            println!(
                "{{\"kind\":\"doc\",\"tag\":\"{tag}\",\"open_ok\":false,\"err\":\"{}\"}}",
                esc(&e)
            );
            return Ok(());
        }
    };
    let open_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let npages = pdf.pages().len();
    if pages.is_empty() {
        pages = (1..=npages).collect();
    }
    let rcache = RenderCache::new();
    let icache = InterpreterCache::new();
    for p1 in pages {
        let Some(idx) = p1.checked_sub(1) else {
            continue;
        };
        let t = Instant::now();
        let r = catch_unwind(AssertUnwindSafe(|| render_page(&pdf, idx, dpi, &rcache)));
        let render_ms = t.elapsed().as_secs_f64() * 1000.0;
        let (ok, err, bmp) = match r {
            Ok(Ok(b)) => (true, String::new(), Some(b)),
            Ok(Err(e)) => (false, e, None),
            Err(_) => (false, "panic".into(), None),
        };
        let mut fine = 0.0f32;
        let (mut bad, mut mad, mut ink_o, mut ink_r, mut mism, mut have_ref) =
            (0.0, 0.0, 0.0, 0.0, false, false);
        if let Some(b) = &bmp {
            if !png.is_empty()
                && std::fs::create_dir_all(&png).is_ok()
                && let Some(enc) = png_encode(b.width, b.height, &b.rgba)
            {
                let _ = std::fs::write(format!("{png}/{tag}-p{p1}.png"), enc);
            }
            if !refdir.is_empty()
                && let Some((rw, rh, rr)) = read_ppm(&format!("{refdir}/{tag}-p{p1}.ppm"))
            {
                have_ref = true;
                (bad, mad, ink_o, ink_r, mism) =
                    compare(&b.rgba, b.width as usize, b.height as usize, rw, rh, &rr);
                fine = fine_bad(&b.rgba, b.width as usize, b.height as usize, rw, rh, &rr);
            }
        }
        let t = Instant::now();
        let tl = catch_unwind(AssertUnwindSafe(|| text_layer(&pdf, idx, &icache)));
        let text_ms = t.elapsed().as_secs_f64() * 1000.0;
        let (text_ok, layer) = match tl {
            Ok(Ok(l)) => (true, l),
            _ => (false, Vec::new()),
        };
        let ours: String = layer.iter().map(|c| c.ch).collect();
        let (mut ref_words, mut found) = (0usize, 0usize);
        let mut hit_total = 0usize;
        let mut hit_found = 0usize;
        if let Ok(rt) = std::fs::read_to_string(format!("{refdir}/{tag}-p{p1}.txt")) {
            let folded = search::fold(&ours);
            for w in words(&rt) {
                ref_words += 1;
                if folded.contains(&w) {
                    found += 1;
                }
            }
            // Known strings: part-number-like tokens (letters and digits mixed, length 4+).
            for tok in rt.split_whitespace() {
                let t: String = tok.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
                if t.len() >= 4
                    && t.chars().any(|c| c.is_ascii_digit())
                    && t.chars().any(|c| c.is_ascii_alphabetic())
                {
                    hit_total += 1;
                    if !search::find(&layer, &t).is_empty() {
                        hit_found += 1;
                    }
                    if hit_total >= 25 {
                        break;
                    }
                }
            }
        }
        let (pw, ph) = datasheet_spike::page_size(&pdf, idx).unwrap_or((0.0, 0.0));
        let blank = bmp
            .as_ref()
            .map(|b| {
                b.rgba
                    .chunks(4)
                    .all(|px| px[0] > 250 && px[1] > 250 && px[2] > 250)
            })
            .unwrap_or(false);
        println!(
            "{{\"kind\":\"page\",\"tag\":\"{tag}\",\"page\":{p1},\"ok\":{ok},\"err\":\"{}\",\"render_ms\":{render_ms:.1},\"w_pt\":{pw:.0},\"h_pt\":{ph:.0},\"have_ref\":{have_ref},\"bad_blocks\":{bad:.4},\"fine_bad\":{fine:.4},\"mad\":{mad:.3},\"ink_ours\":{ink_o:.4},\"ink_ref\":{ink_r:.4},\"size_mismatch\":{mism},\"blank\":{blank},\"text_ok\":{text_ok},\"text_ms\":{text_ms:.1},\"chars\":{},\"ref_words\":{ref_words},\"words_found\":{found},\"hit_total\":{hit_total},\"hit_found\":{hit_found}}}",
            esc(&err),
            layer.len()
        );
    }
    println!(
        "{{\"kind\":\"doc\",\"tag\":\"{tag}\",\"open_ok\":true,\"pages\":{npages},\"open_ms\":{open_ms:.1},\"peak_rss_kb\":{}}}",
        peak_rss_kb()
    );
    Ok(())
}

fn png_encode(w: u32, h: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    use hayro::vello_cpu::Pixmap;
    let mut pm = Pixmap::new(u16::try_from(w).ok()?, u16::try_from(h).ok()?);
    pm.data_as_u8_slice_mut().copy_from_slice(rgba);
    pm.into_png().ok()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let res = match args.first().map(String::as_str) {
        Some("fixtures") => {
            let dir = args.get(1).cloned().unwrap_or_else(|| ".".into());
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string()).and_then(|()| {
                for f in fixtures::all() {
                    std::fs::write(format!("{dir}/{}", f.name), &f.bytes).map_err(|e| e.to_string())?;
                }
                Ok(())
            })
        }
        Some("bench") if args.len() > 1 => bench(&args[1..]),
        _ => Err("usage: spike fixtures <dir> | spike bench <pdf> [--pages 1,2] [--ref dir] [--tag t] [--dpi 150] [--png dir]".into()),
    };
    if let Err(e) = res {
        eprintln!("{e}");
        std::process::exit(2);
    }
}
