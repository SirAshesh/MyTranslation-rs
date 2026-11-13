use image::{Rgb, RgbImage};
use server_rs::model::AppConfig;
use server_rs::pipe::{
    clean_image_text, cluster_lines, ComicRenderer, OcrManager, Translator,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct PageOcrResult {
    page: String,
    width: u32,
    height: u32,
    total_raw_lines: usize,
    total_bubbles: usize,
    bubbles: Vec<BubbleResult>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct BubbleResult {
    id: usize,
    box_rect: [f32; 4],
    text: String,
    line_count: usize,
}

fn list_page_images(folder: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(folder) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let ext_lower = ext.to_lowercase();
                if matches!(ext_lower.as_str(), "jpg" | "jpeg" | "png" | "webp") {
                    files.push(path);
                }
            }
        }
    }
    files.sort_by(|a, b| {
        let extract_num = |p: &Path| -> u32 {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if let Some(inner) = stem
                .rsplit_once('(')
                .and_then(|(_, right)| right.strip_suffix(')'))
            {
                if let Ok(n) = inner.trim().parse::<u32>() {
                    return n;
                }
            }
            stem.rsplit('_')
                .next()
                .or_else(|| stem.rsplit('-').next())
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(0)
        };
        extract_num(a).cmp(&extract_num(b)).then_with(|| a.cmp(b))
    });
    files
}

fn draw_debug_boxes(img: &RgbImage, page_result: &PageOcrResult) -> RgbImage {
    let mut debug_img = img.clone();
    for b in &page_result.bubbles {
        let x1 = (b.box_rect[0] as i32).max(0) as u32;
        let y1 = (b.box_rect[1] as i32).max(0) as u32;
        let x2 = (b.box_rect[2] as i32).min(debug_img.width() as i32 - 1) as u32;
        let y2 = (b.box_rect[3] as i32).min(debug_img.height() as i32 - 1) as u32;

        let stroke_color = Rgb([255, 0, 0]);
        for x in x1..=x2 {
            for dy in 0..2 {
                if y1 + dy < debug_img.height() {
                    debug_img.put_pixel(x, y1 + dy, stroke_color);
                }
                if y2 >= dy && y2 - dy < debug_img.height() {
                    debug_img.put_pixel(x, y2 - dy, stroke_color);
                }
            }
        }
        for y in y1..=y2 {
            for dx in 0..2 {
                if x1 + dx < debug_img.width() {
                    debug_img.put_pixel(x1 + dx, y, stroke_color);
                }
                if x2 >= dx && x2 - dx < debug_img.width() {
                    debug_img.put_pixel(x2 - dx, y, stroke_color);
                }
            }
        }
    }
    debug_img
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let mode = args.get(1).map(|s| s.as_str()).unwrap_or("ocr");
    let input_dir = args.get(2).map(|s| s.as_str()).unwrap_or("dummy/page2");
    let normalized_input = input_dir.replace('\\', "/");
    let out_dir = if normalized_input.contains("dummy/universal") || normalized_input.starts_with("universal") {
        Path::new("dummy/universal/out")
    } else {
        Path::new("dummy/out")
    };

    fs::create_dir_all(out_dir.join("ocr_debug"))?;
    fs::create_dir_all(out_dir.join("inpainting"))?;
    fs::create_dir_all(out_dir.join("translated"))?;

    println!("====================================================");
    println!("Evaluation Runner: Mode '{}', Input '{}'", mode, input_dir);
    println!("====================================================");

    let page_filter = args.get(3).map(|s| s.as_str());

    let mut images = list_page_images(Path::new(input_dir));
    if let Some(target_page) = page_filter {
        images.retain(|p| {
            let stem = p.file_stem().unwrap_or_default().to_string_lossy();
            stem.ends_with(&format!("_{}", target_page))
                || stem.ends_with(&format!("-{}", target_page))
                || stem == target_page
        });
    }
    println!("Found {} matching images in {}", images.len(), input_dir);

    let ocr_mgr = OcrManager::new(51055)?;
    let ocr = Arc::new(Mutex::new(ocr_mgr));

    let config_path = "config.json";
    let cfg = if Path::new(config_path).exists() {
        let raw = fs::read_to_string(config_path)?;
        serde_json::from_str::<AppConfig>(&raw).unwrap_or_default()
    } else {
        AppConfig::default()
    };
    let translator = Translator::new(cfg, PathBuf::from("local"));

    let font_path = "fonts/comic_bold.ttf";
    let renderer = ComicRenderer::new(font_path)?;

    match mode {
        "ocr" => {
            let mut all_results = Vec::new();
            for (idx, img_path) in images.iter().enumerate() {
                let stem = img_path.file_stem().unwrap_or_default().to_string_lossy();
                let img = image::load_from_memory(&fs::read(img_path)?)?.to_rgb8();
                let (w, h) = (img.width(), img.height());

                let raw_lines = {
                    let mut guard = ocr.lock().await;
                    guard.run_ocr(&img)?
                };

                println!("--- Raw OCR lines for {} ({} lines) ---", stem, raw_lines.len());
                for (li, rl) in raw_lines.iter().enumerate() {
                    let xs: Vec<f32> = rl.r#box.iter().map(|p| p[0]).collect();
                    let ys: Vec<f32> = rl.r#box.iter().map(|p| p[1]).collect();
                    let x1 = xs.iter().cloned().fold(f32::INFINITY, f32::min);
                    let x2 = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    let y1 = ys.iter().cloned().fold(f32::INFINITY, f32::min);
                    let y2 = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    println!(
                        "    L#{}: [{:.1}, {:.1} to {:.1}, {:.1}] quad=({:.1},{:.1})({:.1},{:.1})({:.1},{:.1})({:.1},{:.1}) score={:.2}: {}",
                        li, x1, y1, x2, y2,
                        rl.r#box[0][0], rl.r#box[0][1],
                        rl.r#box[1][0], rl.r#box[1][1],
                        rl.r#box[2][0], rl.r#box[2][1],
                        rl.r#box[3][0], rl.r#box[3][1],
                        rl.score, rl.text
                    );
                }

                let bubbles = cluster_lines(raw_lines.clone(), w, h, "en");

                let bubble_results: Vec<BubbleResult> = bubbles
                    .iter()
                    .map(|b| BubbleResult {
                        id: b.id,
                        box_rect: [b.x1, b.y1, b.x2, b.y2],
                        text: b.source_text.clone(),
                        line_count: b.lines.len(),
                    })
                    .collect();

                let page_res = PageOcrResult {
                    page: stem.to_string(),
                    width: w,
                    height: h,
                    total_raw_lines: raw_lines.len(),
                    total_bubbles: bubbles.len(),
                    bubbles: bubble_results,
                };

                let debug_img = draw_debug_boxes(&img, &page_res);
                let debug_out = out_dir.join("ocr_debug").join(format!("{}_ocr.jpg", stem));
                debug_img.save(&debug_out)?;

                println!(
                    "[{}/{}] Page {}: {} raw lines -> {} bubbles",
                    idx + 1,
                    images.len(),
                    stem,
                    page_res.total_raw_lines,
                    page_res.total_bubbles
                );
                for b in &page_res.bubbles {
                    println!("    [#{}] {}", b.id, b.text);
                }

                all_results.push(page_res);
            }

            let json_path = out_dir.join("ocr_results.json");
            fs::write(&json_path, serde_json::to_string_pretty(&all_results)?)?;
            println!("Saved OCR results to {}", json_path.display());
        }
        "inpaint" => {
            for (idx, img_path) in images.iter().enumerate() {
                let stem = img_path.file_stem().unwrap_or_default().to_string_lossy();
                let mut img = image::load_from_memory(&fs::read(img_path)?)?.to_rgb8();
                let (w, h) = (img.width(), img.height());

                let raw_lines = {
                    let mut guard = ocr.lock().await;
                    guard.run_ocr(&img)?
                };

                println!("--- Raw OCR lines for {} ({} lines) ---", stem, raw_lines.len());
                for (li, rl) in raw_lines.iter().enumerate() {
                    let xs: Vec<f32> = rl.r#box.iter().map(|p| p[0]).collect();
                    let ys: Vec<f32> = rl.r#box.iter().map(|p| p[1]).collect();
                    let x1 = xs.iter().cloned().fold(f32::INFINITY, f32::min);
                    let x2 = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    let y1 = ys.iter().cloned().fold(f32::INFINITY, f32::min);
                    let y2 = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    println!(
                        "    L#{}: [{:.1}, {:.1} to {:.1}, {:.1}] quad=({:.1},{:.1})({:.1},{:.1})({:.1},{:.1})({:.1},{:.1}) score={:.2}: {}",
                        li, x1, y1, x2, y2,
                        rl.r#box[0][0], rl.r#box[0][1],
                        rl.r#box[1][0], rl.r#box[1][1],
                        rl.r#box[2][0], rl.r#box[2][1],
                        rl.r#box[3][0], rl.r#box[3][1],
                        rl.score, rl.text
                    );
                }
                let bubbles = cluster_lines(raw_lines, w, h, "en");

                clean_image_text(&mut img, &bubbles);

                let inpaint_out = out_dir.join("inpainting").join(format!("{}_clean.png", stem));
                img.save(&inpaint_out)?;
                println!(
                    "[{}/{}] Cleaned page {}: {} bubbles inpainted -> {}",
                    idx + 1,
                    images.len(),
                    stem,
                    bubbles.len(),
                    inpaint_out.display()
                );
            }
        }
        "full" => {
            for (idx, img_path) in images.iter().enumerate() {
                let stem = img_path.file_stem().unwrap_or_default().to_string_lossy();
                let mut img = image::load_from_memory(&fs::read(img_path)?)?.to_rgb8();
                let (w, h) = (img.width(), img.height());

                let raw_lines = {
                    let mut guard = ocr.lock().await;
                    guard.run_ocr(&img)?
                };

                println!("--- Raw OCR lines for {} ({} lines) ---", stem, raw_lines.len());
                for (li, rl) in raw_lines.iter().enumerate() {
                    let xs: Vec<f32> = rl.r#box.iter().map(|p| p[0]).collect();
                    let ys: Vec<f32> = rl.r#box.iter().map(|p| p[1]).collect();
                    let x1 = xs.iter().cloned().fold(f32::INFINITY, f32::min);
                    let x2 = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    let y1 = ys.iter().cloned().fold(f32::INFINITY, f32::min);
                    let y2 = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
                    println!(
                        "    L#{}: [{:.1}, {:.1} to {:.1}, {:.1}] quad=({:.1},{:.1})({:.1},{:.1})({:.1},{:.1})({:.1},{:.1}) score={:.2}: {}",
                        li, x1, y1, x2, y2,
                        rl.r#box[0][0], rl.r#box[0][1],
                        rl.r#box[1][0], rl.r#box[1][1],
                        rl.r#box[2][0], rl.r#box[2][1],
                        rl.r#box[3][0], rl.r#box[3][1],
                        rl.score, rl.text
                    );
                }
                let bubbles = cluster_lines(raw_lines, w, h, "en");
                if bubbles.is_empty() {
                    println!("[{}/{}] No bubbles for {}", idx + 1, images.len(), stem);
                    continue;
                }

                let translations = translator
                    .translate_bubbles(&bubbles, "en", "id")
                    .await?
                    .translations;

                println!("--- Translations for {} ({} bubbles) ---", stem, bubbles.len());
                for b in &bubbles {
                    let trans = translations.get(&b.id).map(|s| s.as_str()).unwrap_or("");
                    println!("    [#{}] \"{}\" -> \"{}\"", b.id, b.source_text, trans);
                }

                clean_image_text(&mut img, &bubbles);
                renderer.render_bubbles(&mut img, &bubbles, &translations);

                let out_file = out_dir.join("translated").join(format!("{}_id.png", stem));
                img.save(&out_file)?;
                println!(
                    "[{}/{}] Full pipeline completed for {} -> {}",
                    idx + 1,
                    images.len(),
                    stem,
                    out_file.display()
                );
            }
        }
        _ => {
            eprintln!("Unknown mode: {}. Expected 'ocr', 'inpaint', or 'full'.", mode);
        }
    }

    Ok(())
}
