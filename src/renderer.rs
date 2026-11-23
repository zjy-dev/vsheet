use crate::cli::{GridSpec, OutputFormat, Theme};
use crate::extractor::CapturedFrame;
use crate::probe::{format_bitrate, format_duration, format_file_size, VideoMetadata};
use anyhow::{anyhow, Context, Result};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder};
use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{self, fontdb};
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use unicode_width::UnicodeWidthChar;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

const WEB_FONT_FAMILY: &str = "'Microsoft YaHei', '微软雅黑', 'PingFang SC', 'Hiragino Sans GB', 'Heiti SC', 'STHeiti', 'Noto Sans CJK SC', 'Noto Sans SC', 'WenQuanYi Micro Hei', 'Tahoma', 'Verdana', 'Arial', sans-serif";

#[derive(Debug, Clone, Copy)]
pub struct SheetGeometry {
    pub canvas_w: u32,
    pub canvas_h: u32,
    pub scale: f32,
    pub gap: u32,
    pub grid_x: u32,
    pub header_x: u32,
    pub header_y: u32,
    pub header_w: u32,
    pub header_h: u32,
    pub title_lines: usize,
    pub tile_h: f32,
    pub tile_gap: f32,
    pub spec_rows: usize,
    pub grid_y: u32,
    pub cols: u32,
    pub rows: u32,
    pub cell_w: u32,
    pub cell_h: u32,
    pub corner_radius: f32,
}

#[derive(Debug, Clone, Copy)]
struct Palette {
    canvas_bg: &'static str,
    header_bg: &'static str,
    header_border: &'static str,
    tile_bg: &'static str,
    tile_border: &'static str,
    accent: &'static str,
    text_primary: &'static str,
    label_color: &'static str,
    thumb_border: &'static str,
    thumb_inner: &'static str,
    thumb_inner_opacity: f32,
    shadow_opacity: f32,
}

fn palette_for(theme: Theme) -> Palette {
    match theme {
        Theme::Apricot => Palette {
            canvas_bg: "#F8EFE6",
            header_bg: "#FFFDFB",
            header_border: "#E6D3C1",
            tile_bg: "#FDF6EE",
            tile_border: "#EBD9C8",
            accent: "#E67E22",
            text_primary: "#2E1F14",
            label_color: "#9C5216",
            thumb_border: "#D8C2AD",
            thumb_inner: "#000000",
            thumb_inner_opacity: 0.06,
            shadow_opacity: 0.09,
        },
        Theme::Gallery => Palette {
            canvas_bg: "#EFECE6",
            header_bg: "#FFFFFF",
            header_border: "#DCD8D0",
            tile_bg: "#F8F7F4",
            tile_border: "#E4E1DA",
            accent: "#18181B",
            text_primary: "#27272A",
            label_color: "#686660",
            thumb_border: "#CBC7BE",
            thumb_inner: "#000000",
            thumb_inner_opacity: 0.07,
            shadow_opacity: 0.09,
        },
        Theme::Luxury => Palette {
            canvas_bg: "#0D1724",
            header_bg: "#132235",
            header_border: "#283E58",
            tile_bg: "#1A2D44",
            tile_border: "#2E4766",
            accent: "#C69963",
            text_primary: "#F7F4EF",
            label_color: "#D4AA78",
            thumb_border: "#324C6B",
            thumb_inner: "#FFFFFF",
            thumb_inner_opacity: 0.14,
            shadow_opacity: 0.42,
        },
        Theme::Botanical => Palette {
            canvas_bg: "#E9EFE8",
            header_bg: "#FCFDFB",
            header_border: "#CBD9C9",
            tile_bg: "#F3F7F2",
            tile_border: "#D5E2D3",
            accent: "#3D7A5A",
            text_primary: "#1C3026",
            label_color: "#3E6E53",
            thumb_border: "#BCCDBE",
            thumb_inner: "#000000",
            thumb_inner_opacity: 0.06,
            shadow_opacity: 0.09,
        },
        Theme::Ocean => Palette {
            canvas_bg: "#101B27",
            header_bg: "#172636",
            header_border: "#2A4259",
            tile_bg: "#1E3146",
            tile_border: "#314D68",
            accent: "#2EC4B6",
            text_primary: "#F1FAEE",
            label_color: "#76C7C0",
            thumb_border: "#355370",
            thumb_inner: "#FFFFFF",
            thumb_inner_opacity: 0.14,
            shadow_opacity: 0.42,
        },
        Theme::Terracotta => Palette {
            canvas_bg: "#F3E9DC",
            header_bg: "#FFFDF9",
            header_border: "#E2D0BC",
            tile_bg: "#FAF3EA",
            tile_border: "#E7D7C5",
            accent: "#E76F51",
            text_primary: "#264653",
            label_color: "#B5543B",
            thumb_border: "#D5C0A9",
            thumb_inner: "#000000",
            thumb_inner_opacity: 0.06,
            shadow_opacity: 0.09,
        },
        Theme::Rose => Palette {
            canvas_bg: "#F2E6E6",
            header_bg: "#FFFDFD",
            header_border: "#E0CACA",
            tile_bg: "#FAF2F2",
            tile_border: "#E6D3D4",
            accent: "#B56576",
            text_primary: "#431E2F",
            label_color: "#944A5B",
            thumb_border: "#D4BABA",
            thumb_inner: "#000000",
            thumb_inner_opacity: 0.06,
            shadow_opacity: 0.09,
        },
        Theme::Frost => Palette {
            canvas_bg: "#E5EEF7",
            header_bg: "#FFFFFF",
            header_border: "#C6D7EA",
            tile_bg: "#F2F7FC",
            tile_border: "#D2E1F0",
            accent: "#3B6EA5",
            text_primary: "#1B2D42",
            label_color: "#436B99",
            thumb_border: "#B8CCE2",
            thumb_inner: "#000000",
            thumb_inner_opacity: 0.06,
            shadow_opacity: 0.09,
        },
        Theme::Galaxy => Palette {
            canvas_bg: "#151022",
            header_bg: "#1F1832",
            header_border: "#392D59",
            tile_bg: "#292042",
            tile_border: "#443669",
            accent: "#A78BFA",
            text_primary: "#F3EFFB",
            label_color: "#B8A5F2",
            thumb_border: "#4A3B70",
            thumb_inner: "#FFFFFF",
            thumb_inner_opacity: 0.14,
            shadow_opacity: 0.42,
        },
        Theme::Obsidian => Palette {
            canvas_bg: "#111215",
            header_bg: "#1A1C21",
            header_border: "#2E323B",
            tile_bg: "#23262E",
            tile_border: "#363B46",
            accent: "#F59E0B",
            text_primary: "#F3F4F6",
            label_color: "#D99B38",
            thumb_border: "#3E4451",
            thumb_inner: "#FFFFFF",
            thumb_inner_opacity: 0.14,
            shadow_opacity: 0.45,
        },
    }
}

pub fn init_font_database(custom_font: Option<&Path>) -> Arc<fontdb::Database> {
    let mut db = fontdb::Database::new();

    if let Some(font_path) = custom_font {
        if let Err(err) = db.load_font_file(font_path) {
            eprintln!(
                "Warning: failed to load custom font {}: {}",
                font_path.display(),
                err
            );
        }
    }

    // 1. Check Microsoft YaHei if installed on macOS/Windows
    let mut yahei_candidates: Vec<PathBuf> = vec![
        PathBuf::from("/Library/Fonts/Microsoft YaHei.ttf"),
        PathBuf::from("/Library/Fonts/msyh.ttc"),
        PathBuf::from("C:\\Windows\\Fonts\\msyh.ttc"),
        PathBuf::from("C:\\Windows\\Fonts\\msyhbd.ttc"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let h = PathBuf::from(home);
        yahei_candidates.push(h.join("Library/Fonts/Microsoft YaHei.ttf"));
        yahei_candidates.push(h.join("Library/Fonts/msyh.ttc"));
    }
    for p in &yahei_candidates {
        if p.exists() {
            let _ = db.load_font_file(p);
        }
    }

    // 2. Load macOS static PingFang SC (AssetsV2) without loading variable PingFangUI.ttc / SFNS.ttf
    load_macos_static_pingfang(&mut db);

    // 3. Static web-style sans & CJK fallbacks
    let candidate_fonts = [
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Medium.ttc",
        "/System/Library/Fonts/Supplemental/Tahoma.ttf",
        "/System/Library/Fonts/Supplemental/Tahoma Bold.ttf",
        "/System/Library/Fonts/Supplemental/Verdana.ttf",
        "/System/Library/Fonts/Supplemental/Verdana Bold.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        // Linux common CJK & web sans fonts
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansSC-Regular.otf",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        // Windows fallbacks
        "C:\\Windows\\Fonts\\tahoma.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
    ];

    for path_str in candidate_fonts {
        let p = Path::new(path_str);
        if p.exists() {
            let _ = db.load_font_file(p);
        }
    }

    if db.faces().count() < 2 {
        db.load_system_fonts();
    }

    Arc::new(db)
}

fn load_macos_static_pingfang(db: &mut fontdb::Database) {
    let assets_root = Path::new("/System/Library/AssetsV2");
    let Ok(entries) = fs::read_dir(assets_root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !name_str.starts_with("com_apple_MobileAsset_Font") {
            continue;
        }
        let Ok(sub_entries) = fs::read_dir(entry.path()) else {
            continue;
        };
        for sub in sub_entries.flatten() {
            let pf_path = sub.path().join("AssetData/PingFang.ttc");
            if pf_path.is_file() {
                let _ = db.load_font_file(&pf_path);
                return;
            }
        }
    }
}

pub fn compute_geometry(
    canvas_w: u32,
    padding: u32,
    gap: u32,
    grid: GridSpec,
    meta: &VideoMetadata,
) -> SheetGeometry {
    let scale = (canvas_w as f32 / 1920.0).clamp(0.75, 2.0);
    let outer_pad = padding;

    let total_gap_w = gap * (grid.cols.saturating_sub(1));
    let avail_w = canvas_w.saturating_sub(2 * outer_pad + total_gap_w);
    let cell_w = ((avail_w / grid.cols).max(2)) & !1;
    let grid_w = cell_w * grid.cols + total_gap_w;
    let grid_x = (canvas_w.saturating_sub(grid_w)) / 2;

    let aspect = (meta.display_height as f64 / meta.display_width as f64).clamp(0.25, 2.5);
    let cell_h = ((cell_w as f64 * aspect).round() as u32).max(2) & !1;

    let header_pad_x = (18.0 * scale).round();
    let header_pad_y = (15.0 * scale).round();
    let title_avail_w = (grid_w as f32 - header_pad_x * 2.0 - 12.0 * scale).max(200.0);
    let title_fs = 23.0 * scale;
    let max_units = ((title_avail_w / (title_fs * 0.56)).floor() as usize).max(16);
    let wrapped = wrap_title_lines(&meta.file_name, max_units, 2);
    let title_lines = wrapped.len().max(1);

    let title_block_h = if title_lines == 1 {
        28.0 * scale
    } else {
        56.0 * scale
    };
    let title_to_tiles_gap = (12.0 * scale).round();
    let spec_rows: usize = if canvas_w >= 1360 { 1 } else { 2 };
    let tile_h = (68.0 * scale).round();
    let tile_gap = (8.0 * scale).round();
    let tiles_total_h = (spec_rows as f32) * tile_h + (spec_rows.saturating_sub(1) as f32) * tile_gap;
    let header_h = (header_pad_y * 2.0 + title_block_h + title_to_tiles_gap + tiles_total_h).round() as u32;

    let header_x = grid_x;
    let header_y = outer_pad;
    let header_w = grid_w;
    let header_gap = gap.max((12.0 * scale).round() as u32);
    let grid_y = header_y + header_h + header_gap;

    let grid_h = cell_h * grid.rows + gap * (grid.rows.saturating_sub(1));
    let canvas_h = grid_y + grid_h + outer_pad;
    let corner_radius = if gap == 0 {
        0.0
    } else {
        (3.0 * scale).clamp(2.0, 5.0)
    };

    SheetGeometry {
        canvas_w,
        canvas_h,
        scale,
        gap,
        grid_x,
        header_x,
        header_y,
        header_w,
        header_h,
        title_lines,
        tile_h,
        tile_gap,
        spec_rows,
        grid_y,
        cols: grid.cols,
        rows: grid.rows,
        cell_w,
        cell_h,
        corner_radius,
    }
}

pub fn render_and_save(
    output_path: &Path,
    format: OutputFormat,
    quality: u8,
    theme: Theme,
    geom: &SheetGeometry,
    meta: &VideoMetadata,
    frames: &[CapturedFrame],
    show_timestamps: bool,
    fontdb: &Arc<fontdb::Database>,
) -> Result<()> {
    let pal = palette_for(theme);

    let mut pixmap = Pixmap::new(geom.canvas_w, geom.canvas_h)
        .ok_or_else(|| anyhow!("Failed allocating {}x{} pixmap", geom.canvas_w, geom.canvas_h))?;

    // Pass 1: Render flat background and two-tier structured specification table header.
    let base_svg = build_base_svg(geom, meta, pal);
    render_svg_into_pixmap(&base_svg, fontdb, &mut pixmap)
        .context("Failed rendering contact sheet base SVG layer")?;

    // Pass 2: Blit RGB24 thumbnails directly into grid cells.
    blit_thumbnails(&mut pixmap, geom, frames);

    // Pass 3: Render subtle cell borders and readable timestamp tags.
    let overlay_svg = build_overlay_svg(geom, frames, pal, show_timestamps);
    render_svg_into_pixmap(&overlay_svg, fontdb, &mut pixmap)
        .context("Failed rendering contact sheet overlay SVG layer")?;

    // Encode and write atomically via temp file + rename.
    save_pixmap_atomic(&pixmap, output_path, format, quality)
}

fn render_svg_into_pixmap(
    svg_str: &str,
    fontdb: &Arc<fontdb::Database>,
    pixmap: &mut Pixmap,
) -> Result<()> {
    let mut opt = usvg::Options::default();
    opt.fontdb = Arc::clone(fontdb);
    let tree = usvg::Tree::from_str(svg_str, &opt)
        .map_err(|e| anyhow!("SVG parse error: {}", e))?;
    resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());
    Ok(())
}

fn build_meta_items(meta: &VideoMetadata) -> Vec<(&'static str, String)> {
    let duration_val = if meta.duration_secs > 0.0 {
        format_duration(meta.duration_secs)
    } else {
        "-".to_string()
    };

    let res_val = format!(
        "{} × {} ({})",
        meta.display_width,
        meta.display_height,
        meta.aspect_ratio_label()
    );

    let size_val = if meta.file_size_bytes > 0 {
        format_file_size(meta.file_size_bytes)
    } else {
        "-".to_string()
    };

    let bitrate_val = meta
        .bitrate_bps
        .map(format_bitrate)
        .unwrap_or_else(|| "-".to_string());

    let mut video_parts = vec![meta.video_codec.clone()];
    if let Some(fps) = meta.fps {
        if (fps - fps.round()).abs() < 0.05 {
            video_parts.push(format!("{:.0} fps", fps));
        } else {
            video_parts.push(format!("{:.2} fps", fps));
        }
    }
    if let Some(depth) = meta.bit_depth {
        video_parts.push(format!("{}-bit", depth));
    }
    if let Some(ref hdr) = meta.hdr_label {
        video_parts.push(hdr.clone());
    }
    let video_val = video_parts.join(" · ");

    let audio_val = match &meta.audio_codec {
        Some(codec) => {
            let mut parts = vec![codec.clone()];
            if let Some(sr) = meta.audio_sample_rate_hz {
                parts.push(format!("{:.1} kHz", sr as f64 / 1000.0));
            }
            if let Some(ref ch) = meta.audio_channels_label {
                parts.push(ch.clone());
            }
            parts.join(" · ")
        }
        None => "无音轨".to_string(),
    };

    vec![
        ("视频时长", duration_val),
        ("分辨率", res_val),
        ("文件大小", size_val),
        ("平均码率", bitrate_val),
        ("视频编码", video_val),
        ("音频信息", audio_val),
    ]
}

fn build_base_svg(geom: &SheetGeometry, meta: &VideoMetadata, pal: Palette) -> String {
    let s = geom.scale;
    let card_r = (6.0 * s).clamp(4.0, 8.0);
    let mut svg = String::with_capacity(8192);

    let hx = geom.header_x as f32;
    let hy = geom.header_y as f32;
    let hw = geom.header_w as f32;
    let hh = geom.header_h as f32;

    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
  <rect width="{w}" height="{h}" fill="{bg}"/>
  <rect x="{hx:.1}" y="{sy:.1}" width="{hw:.1}" height="{hh:.1}" rx="{cr:.1}" fill="#000000" fill-opacity="{sho:.2}"/>
  <rect x="{hx:.1}" y="{hy:.1}" width="{hw:.1}" height="{hh:.1}" rx="{cr:.1}" fill="{hbg}" stroke="{hbd}" stroke-width="1"/>
"##,
        w = geom.canvas_w,
        h = geom.canvas_h,
        bg = pal.canvas_bg,
        hx = hx,
        hy = hy,
        sy = hy + 2.5 * s,
        hw = hw,
        hh = hh,
        cr = card_r,
        sho = pal.shadow_opacity * 0.75,
        hbg = pal.header_bg,
        hbd = pal.header_border,
    ));

    // Upper Tier: Accent Bar + Video Filename
    let pad_x = (18.0 * s).round();
    let pad_y = (15.0 * s).round();
    let tiles_left = hx + pad_x;
    let title_x = tiles_left + 12.0 * s;
    let title_avail_w = (hw - pad_x * 2.0 - 12.0 * s).max(200.0);
    let title_fs = 23.0 * s;
    let max_units = ((title_avail_w / (title_fs * 0.56)).floor() as usize).max(16);
    let lines = wrap_title_lines(&meta.file_name, max_units, 2);

    let bar_w = (4.0 * s).clamp(3.0, 6.0);
    let bar_r = (2.0 * s).clamp(1.5, 3.0);
    if lines.len() <= 1 {
        let line = lines.first().map(String::as_str).unwrap_or("");
        let bar_h = 22.0 * s;
        let bar_y = hy + pad_y + 2.5 * s;
        let ty = hy + pad_y + 21.0 * s;
        svg.push_str(&format!(
            r#"  <rect x="{bx:.1}" y="{by:.1}" width="{bw:.1}" height="{bh:.1}" rx="{br:.1}" fill="{ac}"/>
  <text x="{tx:.1}" y="{ty:.1}" font-family="{ff}" font-size="{fs:.1}" font-weight="700" fill="{tc}">{txt}</text>
"#,
            bx = tiles_left,
            by = bar_y,
            bw = bar_w,
            bh = bar_h,
            br = bar_r,
            ac = pal.accent,
            tx = title_x,
            ty = ty,
            ff = WEB_FONT_FAMILY,
            fs = title_fs,
            tc = pal.text_primary,
            txt = xml_escape(line),
        ));
    } else {
        let bar_h = 46.0 * s;
        let bar_y = hy + pad_y + 3.5 * s;
        let ty1 = hy + pad_y + 20.0 * s;
        let ty2 = hy + pad_y + 48.0 * s;
        svg.push_str(&format!(
            r#"  <rect x="{bx:.1}" y="{by:.1}" width="{bw:.1}" height="{bh:.1}" rx="{br:.1}" fill="{ac}"/>
  <text x="{tx:.1}" y="{ty1:.1}" font-family="{ff}" font-size="{fs:.1}" font-weight="700" fill="{tc}">{t1}</text>
  <text x="{tx:.1}" y="{ty2:.1}" font-family="{ff}" font-size="{fs:.1}" font-weight="700" fill="{tc}">{t2}</text>
"#,
            bx = tiles_left,
            by = bar_y,
            bw = bar_w,
            bh = bar_h,
            br = bar_r,
            ac = pal.accent,
            tx = title_x,
            ty1 = ty1,
            ty2 = ty2,
            ff = WEB_FONT_FAMILY,
            fs = title_fs * 0.92,
            tc = pal.text_primary,
            t1 = xml_escape(&lines[0]),
            t2 = xml_escape(&lines[1]),
        ));
    }

    // Lower Tier: 6 Clearly Bounded Cohesive Metadata Tiles (1x6 wide or 2x3 narrow)
    let items = build_meta_items(meta);
    let title_block_h = if geom.title_lines <= 1 {
        28.0 * s
    } else {
        56.0 * s
    };
    let title_to_tiles_gap = (12.0 * s).round();
    let tiles_top = hy + pad_y + title_block_h + title_to_tiles_gap;
    let tiles_total_w = hw - pad_x * 2.0;
    let tile_h = geom.tile_h;
    let tile_gap = geom.tile_gap;
    let tile_r = (4.5 * s).clamp(3.0, 6.0);
    let tile_pad_x = 15.0 * s;
    let label_fs = 13.5 * s;
    let base_val_fs = 19.0 * s;

    if geom.spec_rows == 1 {
        let weights: [f32; 6] = [0.13, 0.20, 0.13, 0.14, 0.20, 0.20];
        let avail_tw = tiles_total_w - 5.0 * tile_gap;
        let mut cur_x = tiles_left;
        for ((label, value), &wt) in items.iter().zip(weights.iter()) {
            let cw = avail_tw * wt;
            render_spec_tile(
                &mut svg,
                cur_x,
                tiles_top,
                cw,
                tile_h,
                tile_r,
                tile_pad_x,
                s,
                label_fs,
                base_val_fs,
                label,
                value,
                pal,
            );
            cur_x += cw + tile_gap;
        }
    } else {
        let weights: [f32; 3] = [0.28, 0.36, 0.36];
        let avail_tw = tiles_total_w - 2.0 * tile_gap;
        for row_idx in 0..2 {
            let row_y = tiles_top + (row_idx as f32) * (tile_h + tile_gap);
            let mut cur_x = tiles_left;
            for col_idx in 0..3 {
                let item_idx = row_idx * 3 + col_idx;
                let (label, value) = &items[item_idx];
                let cw = avail_tw * weights[col_idx];
                render_spec_tile(
                    &mut svg,
                    cur_x,
                    row_y,
                    cw,
                    tile_h,
                    tile_r,
                    tile_pad_x,
                    s,
                    label_fs,
                    base_val_fs,
                    label,
                    value,
                    pal,
                );
                cur_x += cw + tile_gap;
            }
        }
    }

    // Subtle elevation shadows under every thumbnail cell
    let shadow_dy = 2.5 * s;
    for row in 0..geom.rows {
        for col in 0..geom.cols {
            let cx = geom.grid_x + col * (geom.cell_w + geom.gap);
            let cy = geom.grid_y + row * (geom.cell_h + geom.gap);
            svg.push_str(&format!(
                r##"  <rect x="{cx}" y="{sy:.1}" width="{cw}" height="{ch}" rx="{r:.1}" fill="#000000" fill-opacity="{so:.2}"/>
"##,
                cx = cx,
                sy = cy as f32 + shadow_dy,
                cw = geom.cell_w,
                ch = geom.cell_h,
                r = geom.corner_radius,
                so = pal.shadow_opacity,
            ));
        }
    }

    svg.push_str("</svg>\n");
    svg
}

#[allow(clippy::too_many_arguments)]
fn render_spec_tile(
    svg: &mut String,
    tile_x: f32,
    tile_y: f32,
    tile_w: f32,
    tile_h: f32,
    tile_r: f32,
    pad_x: f32,
    s: f32,
    label_fs: f32,
    base_val_fs: f32,
    label: &str,
    value: &str,
    pal: Palette,
) {
    svg.push_str(&format!(
        r#"  <rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="{r:.1}" fill="{bg}" stroke="{bd}" stroke-width="1"/>
"#,
        x = tile_x,
        y = tile_y,
        w = tile_w,
        h = tile_h,
        r = tile_r,
        bg = pal.tile_bg,
        bd = pal.tile_border,
    ));

    let tx = tile_x + pad_x;
    let label_y = tile_y + 24.0 * s;
    let val_y = tile_y + 51.0 * s;

    let avail_val_w = (tile_w - pad_x * 1.8).max(40.0 * s);
    let val_units = display_width_units(value) as f32;
    let est_w = val_units * base_val_fs * 0.54;
    let val_fs = if est_w > avail_val_w {
        (base_val_fs * (avail_val_w / est_w)).max(11.0 * s)
    } else {
        base_val_fs
    };

    svg.push_str(&format!(
        r#"  <text x="{tx:.1}" y="{ly:.1}" font-family="{ff}" font-size="{lfs:.1}" font-weight="600" fill="{lc}">{lbl}</text>
  <text x="{tx:.1}" y="{vy:.1}" font-family="{ff}" font-size="{vfs:.1}" font-weight="600" fill="{vc}">{val}</text>
"#,
        tx = tx,
        ly = label_y,
        vy = val_y,
        ff = WEB_FONT_FAMILY,
        lfs = label_fs,
        vfs = val_fs,
        lc = pal.label_color,
        vc = pal.text_primary,
        lbl = xml_escape(label),
        val = xml_escape(value),
    ));
}

fn build_overlay_svg(
    geom: &SheetGeometry,
    frames: &[CapturedFrame],
    pal: Palette,
    show_timestamps: bool,
) -> String {
    let mut svg = String::with_capacity(6144);
    svg.push_str(&format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">
"#,
        w = geom.canvas_w,
        h = geom.canvas_h,
    ));

    let outer_r = if geom.corner_radius > 0.0 {
        geom.corner_radius + 0.5
    } else {
        0.0
    };
    let cell_scale = (geom.cell_w as f32 / 460.0).clamp(0.68, 1.20);
    let pill_w = (76.0 * cell_scale).round();
    let pill_h = (23.0 * cell_scale).round();
    let pill_r = 3.0 * cell_scale;
    let pill_pad = (6.0 * cell_scale).round().max(4.0);
    let ts_fs = (13.5 * cell_scale).max(9.5);

    for frame in frames {
        let idx = frame.index as u32;
        let col = idx % geom.cols;
        let row = idx / geom.cols;
        if row >= geom.rows {
            continue;
        }
        let x = (geom.grid_x + col * (geom.cell_w + geom.gap)) as f32;
        let y = (geom.grid_y + row * (geom.cell_h + geom.gap)) as f32;
        let cw = geom.cell_w as f32;
        let ch = geom.cell_h as f32;

        // Crisp 1px hue-harmonized outer border + subtle inner contrast hairline
        svg.push_str(&format!(
            r#"  <rect x="{ox:.2}" y="{oy:.2}" width="{ow:.2}" height="{oh:.2}" rx="{or:.2}" fill="none" stroke="{obc}" stroke-width="1"/>
  <rect x="{ix:.2}" y="{iy:.2}" width="{iw:.2}" height="{ih:.2}" rx="{ir:.2}" fill="none" stroke="{ibc}" stroke-opacity="{ibo:.2}" stroke-width="1"/>
"#,
            ox = x - 0.5,
            oy = y - 0.5,
            ow = cw + 1.0,
            oh = ch + 1.0,
            or = outer_r,
            obc = pal.thumb_border,
            ix = x + 0.5,
            iy = y + 0.5,
            iw = (cw - 1.0).max(1.0),
            ih = (ch - 1.0).max(1.0),
            ir = geom.corner_radius,
            ibc = pal.thumb_inner,
            ibo = pal.thumb_inner_opacity,
        ));

        if show_timestamps && cw > pill_w + pill_pad * 1.5 && ch > pill_h + pill_pad * 1.5 {
            let px = x + cw - pill_w - pill_pad;
            let py = y + ch - pill_h - pill_pad;
            let tx = px + pill_w * 0.5;
            let ty = py + pill_h * 0.72;
            let ts_str = format_duration(frame.timestamp_secs);

            svg.push_str(&format!(
                r##"  <rect x="{px:.1}" y="{py:.1}" width="{pw:.1}" height="{ph:.1}" rx="{pr:.1}" fill="#000000" fill-opacity="0.74" stroke="#FFFFFF" stroke-opacity="0.24" stroke-width="1"/>
  <text x="{tx:.1}" y="{ty:.1}" text-anchor="middle" font-family="{ff}" font-size="{fs:.1}" font-weight="600" fill="#FFFFFF">{ts}</text>
"##,
                px = px,
                py = py,
                pw = pill_w,
                ph = pill_h,
                pr = pill_r,
                tx = tx,
                ty = ty,
                ff = WEB_FONT_FAMILY,
                fs = ts_fs,
                ts = ts_str,
            ));
        }
    }

    svg.push_str("</svg>\n");
    svg
}

fn blit_thumbnails(pixmap: &mut Pixmap, geom: &SheetGeometry, frames: &[CapturedFrame]) {
    let canvas_w = pixmap.width() as usize;
    let canvas_h = pixmap.height() as usize;
    let dst = pixmap.data_mut();
    let cell_w = geom.cell_w as usize;
    let cell_h = geom.cell_h as usize;
    let r = geom.corner_radius;
    let r_ceil = if r <= 0.0 {
        0
    } else {
        (r.ceil() as usize + 1).min(cell_w / 2).min(cell_h / 2)
    };

    for frame in frames {
        let idx = frame.index as u32;
        let col = idx % geom.cols;
        let row = idx / geom.cols;
        if row >= geom.rows {
            continue;
        }

        let ox = (geom.grid_x + col * (geom.cell_w + geom.gap)) as usize;
        let oy = (geom.grid_y + row * (geom.cell_h + geom.gap)) as usize;
        let src = &frame.rgb24;
        if src.len() < cell_w * cell_h * 3 {
            continue;
        }

        for ly in 0..cell_h {
            let py = oy + ly;
            if py >= canvas_h {
                break;
            }
            let in_top = ly < r_ceil;
            let in_bottom = ly >= cell_h - r_ceil;

            let src_row = ly * cell_w * 3;
            let dst_row = py * canvas_w * 4;

            if !in_top && !in_bottom {
                let copy_w = cell_w.min(canvas_w.saturating_sub(ox));
                for lx in 0..copy_w {
                    let s_idx = src_row + lx * 3;
                    let d_idx = dst_row + (ox + lx) * 4;
                    dst[d_idx] = src[s_idx];
                    dst[d_idx + 1] = src[s_idx + 1];
                    dst[d_idx + 2] = src[s_idx + 2];
                    dst[d_idx + 3] = 255;
                }
                continue;
            }

            let cy = if in_top {
                r
            } else {
                cell_h as f32 - r
            };
            let py_f = ly as f32 + 0.5;
            let dy = (py_f - cy).abs();

            let copy_w = cell_w.min(canvas_w.saturating_sub(ox));
            for lx in 0..copy_w {
                let s_idx = src_row + lx * 3;
                let d_idx = dst_row + (ox + lx) * 4;

                let in_left = lx < r_ceil;
                let in_right = lx >= cell_w - r_ceil;

                if !in_left && !in_right {
                    dst[d_idx] = src[s_idx];
                    dst[d_idx + 1] = src[s_idx + 1];
                    dst[d_idx + 2] = src[s_idx + 2];
                    dst[d_idx + 3] = 255;
                    continue;
                }

                let cx = if in_left {
                    r
                } else {
                    cell_w as f32 - r
                };
                let dx = ((lx as f32 + 0.5) - cx).abs();
                let dist = (dx * dx + dy * dy).sqrt() - r;
                let cov = (0.5 - dist).clamp(0.0, 1.0);

                if cov <= 0.0 {
                    continue;
                }
                if cov >= 1.0 {
                    dst[d_idx] = src[s_idx];
                    dst[d_idx + 1] = src[s_idx + 1];
                    dst[d_idx + 2] = src[s_idx + 2];
                    dst[d_idx + 3] = 255;
                } else {
                    let inv = 1.0 - cov;
                    dst[d_idx] = (src[s_idx] as f32 * cov + dst[d_idx] as f32 * inv).round() as u8;
                    dst[d_idx + 1] =
                        (src[s_idx + 1] as f32 * cov + dst[d_idx + 1] as f32 * inv).round() as u8;
                    dst[d_idx + 2] =
                        (src[s_idx + 2] as f32 * cov + dst[d_idx + 2] as f32 * inv).round() as u8;
                    dst[d_idx + 3] = 255;
                }
            }
        }
    }
}

fn save_pixmap_atomic(
    pixmap: &Pixmap,
    output_path: &Path,
    format: OutputFormat,
    quality: u8,
) -> Result<()> {
    let w = pixmap.width();
    let h = pixmap.height();
    let rgba = pixmap.data();
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for px in rgba.chunks_exact(4) {
        rgb.push(px[0]);
        rgb.push(px[1]);
        rgb.push(px[2]);
    }

    let parent = match output_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    fs::create_dir_all(parent).with_context(|| {
        format!("Failed creating output directory {}", parent.display())
    })?;
    let stem = output_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("sheet");
    let seq = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_name = format!(
        ".{}.{}.vsheet.tmp.{}.{}",
        stem,
        format.ext(),
        std::process::id(),
        seq
    );
    let temp_path: PathBuf = parent.join(temp_name);

    let encode_result = (|| -> Result<()> {
        let file = File::create(&temp_path).with_context(|| {
            format!("Failed creating temp file {}", temp_path.display())
        })?;
        let mut writer = BufWriter::with_capacity(256 * 1024, file);
        match format {
            OutputFormat::Jpg => {
                let mut encoder = JpegEncoder::new_with_quality(&mut writer, quality);
                encoder
                    .encode(&rgb, w, h, ColorType::Rgb8.into())
                    .context("JPEG encoding failed")?;
            }
            OutputFormat::Png => {
                let encoder = PngEncoder::new(&mut writer);
                encoder
                    .write_image(&rgb, w, h, ColorType::Rgb8.into())
                    .context("PNG encoding failed")?;
            }
        }
        use std::io::Write;
        writer.flush()?;
        Ok(())
    })();

    if let Err(err) = encode_result {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }

    if let Err(err) = fs::rename(&temp_path, output_path) {
        let _ = fs::remove_file(&temp_path);
        return Err(err).with_context(|| {
            format!(
                "Failed renaming temp file {} to {}",
                temp_path.display(),
                output_path.display()
            )
        });
    }

    Ok(())
}

pub fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() && c != '\t' && c != '\n' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}

pub fn display_width_units(s: &str) -> usize {
    s.chars().map(|c| c.width().unwrap_or(1)).sum()
}

pub fn wrap_title_lines(title: &str, max_units_per_line: usize, max_lines: usize) -> Vec<String> {
    let max_units = max_units_per_line.max(6);
    if display_width_units(title) <= max_units || max_lines <= 1 {
        if display_width_units(title) <= max_units {
            return vec![title.to_string()];
        }
        return vec![middle_truncate_units(title, max_units)];
    }

    let mut line1 = String::new();
    let mut used = 0usize;
    let mut split_byte = title.len();

    for (byte_idx, ch) in title.char_indices() {
        let cw = ch.width().unwrap_or(1);
        if used + cw > max_units {
            split_byte = byte_idx;
            break;
        }
        line1.push(ch);
        used += cw;
    }

    let rest = title[split_byte..].trim_start();
    if rest.is_empty() {
        return vec![line1];
    }

    let line2 = if display_width_units(rest) <= max_units {
        rest.to_string()
    } else {
        middle_truncate_units(rest, max_units)
    };

    vec![line1, line2]
}

fn middle_truncate_units(s: &str, max_units: usize) -> String {
    if display_width_units(s) <= max_units {
        return s.to_string();
    }
    if max_units <= 3 {
        return "…".to_string();
    }
    let budget = max_units - 1;
    let head_budget = budget / 2;
    let tail_budget = budget - head_budget;

    let mut head = String::new();
    let mut hu = 0usize;
    for ch in s.chars() {
        let w = ch.width().unwrap_or(1);
        if hu + w > head_budget {
            break;
        }
        head.push(ch);
        hu += w;
    }

    let mut tail_chars: Vec<char> = Vec::new();
    let mut tu = 0usize;
    for ch in s.chars().rev() {
        let w = ch.width().unwrap_or(1);
        if tu + w > tail_budget {
            break;
        }
        tail_chars.push(ch);
        tu += w;
    }
    tail_chars.reverse();
    let tail: String = tail_chars.into_iter().collect();

    format!("{}…{}", head, tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_handles_special_chars() {
        assert_eq!(
            xml_escape("A & B <Test> \"1\" '2'"),
            "A &amp; B &lt;Test&gt; &quot;1&quot; &apos;2&apos;"
        );
    }

    #[test]
    fn cjk_title_wraps_and_truncates_cleanly() {
        let short = "自然风光纪录片.mp4";
        assert_eq!(wrap_title_lines(short, 40, 2), vec![short.to_string()]);

        let long = "这是一段非常非常长的中文视频标题用来测试两行换行以及中间省略号截断效果_4K_HDR_HEVC.mp4";
        let lines = wrap_title_lines(long, 30, 2);
        assert_eq!(lines.len(), 2);
        assert!(display_width_units(&lines[0]) <= 30);
        assert!(display_width_units(&lines[1]) <= 30);
    }
}
