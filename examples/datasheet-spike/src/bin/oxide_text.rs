//! Text-layer comparison binary using pdf_oxide (spike #723, feature `oxide`).
//!
//! Context: same inputs and the same poppler reference text as `spike bench`, so the two text layers
//! can be compared on word coverage, part-number hit rate and time per page. One JSON line per page.
//!
//! Do not: use for rendering; pdf_oxide is only a text candidate here.

use pdf_oxide::PdfDocument;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

fn fold(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 4 {
        eprintln!("usage: oxide-text <pdf> <tag> <refdir> <pages csv>");
        std::process::exit(2);
    }
    let (path, tag, refdir) = (&a[0], &a[1], &a[2]);
    let pages: Vec<usize> = a[3].split(',').filter_map(|p| p.parse().ok()).collect();
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return,
    };
    let doc = match catch_unwind(AssertUnwindSafe(|| PdfDocument::from_bytes(bytes))) {
        Ok(Ok(d)) => d,
        _ => {
            println!("{{\"kind\":\"oxide_doc\",\"tag\":\"{tag}\",\"open_ok\":false}}");
            return;
        }
    };
    for p1 in pages {
        let t = Instant::now();
        let r = catch_unwind(AssertUnwindSafe(|| doc.extract_chars(p1 - 1)));
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let chars = match r {
            Ok(Ok(c)) => c,
            _ => {
                println!(
                    "{{\"kind\":\"oxide_page\",\"tag\":\"{tag}\",\"page\":{p1},\"ok\":false,\"ms\":{ms:.1}}}"
                );
                continue;
            }
        };
        let text: String = chars.iter().map(|c| c.char).collect();
        let folded = fold(&text);
        let (mut rw, mut found, mut ht, mut hf) = (0, 0, 0, 0);
        if let Ok(rt) = std::fs::read_to_string(format!("{refdir}/{tag}-p{p1}.txt")) {
            let mut ws: Vec<String> = rt
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| w.chars().count() >= 3)
                .map(str::to_lowercase)
                .collect();
            ws.sort();
            ws.dedup();
            for w in ws {
                rw += 1;
                if folded.contains(&w) {
                    found += 1;
                }
            }
            for tok in rt.split_whitespace() {
                let t: String = tok.trim_matches(|c: char| !c.is_alphanumeric()).to_string();
                if t.len() >= 4
                    && t.chars().any(|c| c.is_ascii_digit())
                    && t.chars().any(|c| c.is_ascii_alphabetic())
                {
                    ht += 1;
                    if folded.contains(&fold(&t)) {
                        hf += 1;
                    }
                    if ht >= 25 {
                        break;
                    }
                }
            }
        }
        println!(
            "{{\"kind\":\"oxide_page\",\"tag\":\"{tag}\",\"page\":{p1},\"ok\":true,\"ms\":{ms:.1},\"chars\":{},\"ref_words\":{rw},\"words_found\":{found},\"hit_total\":{ht},\"hit_found\":{hf}}}",
            chars.len()
        );
    }
}
