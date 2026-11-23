use crate::cli::GridSpec;
use crate::probe::VideoMetadata;
use anyhow::{bail, Context, Result};
use rayon::prelude::*;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Clone)]
pub struct SmartFrameConfig {
    pub enabled: bool,
    pub blank_step: Option<f64>,
    pub blank_retries: u32,
    pub black_threshold: f32,
    pub white_threshold: f32,
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlankKind {
    Black,
    White,
    Solid,
}

#[derive(Debug, Clone, Copy)]
pub struct FrameAnalysis {
    pub is_blank: bool,
    #[cfg_attr(not(test), allow(dead_code))]
    pub blank_kind: Option<BlankKind>,
    pub quality_score: f32,
}

#[derive(Debug, Clone)]
pub struct CapturedFrame {
    pub index: usize,
    pub timestamp_secs: f64,
    pub rgb24: Vec<u8>,
    pub shifted_from_blank: bool,
}

#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub frames: Vec<CapturedFrame>,
    pub shifted_count: usize,
}

pub fn extract_frames(
    video_path: &Path,
    meta: &VideoMetadata,
    grid: GridSpec,
    thumb_w: u32,
    thumb_h: u32,
    smart: &SmartFrameConfig,
    parallel_cells: bool,
) -> Result<ExtractionResult> {
    let total_cells = grid.total_cells() as usize;
    let indices: Vec<usize> = (0..total_cells).collect();

    let frames: Vec<CapturedFrame> = if parallel_cells && total_cells > 1 {
        indices
            .into_par_iter()
            .map(|idx| {
                capture_cell(
                    video_path,
                    meta,
                    idx,
                    total_cells,
                    thumb_w,
                    thumb_h,
                    smart,
                )
            })
            .collect::<Result<Vec<_>>>()?
    } else {
        indices
            .into_iter()
            .map(|idx| {
                capture_cell(
                    video_path,
                    meta,
                    idx,
                    total_cells,
                    thumb_w,
                    thumb_h,
                    smart,
                )
            })
            .collect::<Result<Vec<_>>>()?
    };

    let shifted_count = frames.iter().filter(|f| f.shifted_from_blank).count();
    Ok(ExtractionResult {
        frames,
        shifted_count,
    })
}

fn capture_cell(
    video_path: &Path,
    meta: &VideoMetadata,
    cell_index: usize,
    total_cells: usize,
    thumb_w: u32,
    thumb_h: u32,
    smart: &SmartFrameConfig,
) -> Result<CapturedFrame> {
    let duration = meta.duration_secs;
    if duration <= 0.05 {
        let (rgb24, actual_ts) = extract_raw_rgb24(
            video_path,
            meta.video_stream_index,
            0.0,
            thumb_w,
            thumb_h,
        )?;
        return Ok(CapturedFrame {
            index: cell_index,
            timestamp_secs: actual_ts,
            rgb24,
            shifted_from_blank: false,
        });
    }

    let slot_span = duration / (total_cells as f64);
    let t_init = (cell_index as f64 * slot_span + slot_span * 0.30).min((duration - 0.1).max(0.0));

    if !smart.enabled || duration <= 0.2 || smart.blank_retries == 0 {
        let (rgb24, actual_ts) = extract_raw_rgb24(
            video_path,
            meta.video_stream_index,
            t_init,
            thumb_w,
            thumb_h,
        )?;
        return Ok(CapturedFrame {
            index: cell_index,
            timestamp_secs: actual_ts,
            rgb24,
            shifted_from_blank: false,
        });
    }

    let max_ts = ((cell_index + 1) as f64 * slot_span - 0.15)
        .max(t_init)
        .min((duration - 0.1).max(0.0));
    let rem = (max_ts - t_init).max(0.0);

    let step = if let Some(user_step) = smart.blank_step {
        let s = user_step.max(0.2);
        if t_init + s > max_ts && rem > 0.05 {
            (rem / smart.blank_retries as f64).max(0.05)
        } else {
            s
        }
    } else {
        (rem / (smart.blank_retries as f64 + 0.5)).max(0.4).min(6.0)
    };

    let mut best_rgb: Option<Vec<u8>> = None;
    let mut best_ts = t_init;
    let mut best_score = f32::NEG_INFINITY;
    let mut best_attempt_idx = 0u32;

    let mut ts = t_init;
    for attempt in 0..=smart.blank_retries {
        let (rgb24, actual_ts) = extract_raw_rgb24(
            video_path,
            meta.video_stream_index,
            ts,
            thumb_w,
            thumb_h,
        )?;
        let analysis = analyze_frame(
            &rgb24,
            thumb_w,
            thumb_h,
            smart.black_threshold,
            smart.white_threshold,
        );

        if !analysis.is_blank {
            return Ok(CapturedFrame {
                index: cell_index,
                timestamp_secs: actual_ts,
                rgb24,
                shifted_from_blank: attempt > 0,
            });
        }

        if analysis.quality_score > best_score {
            best_score = analysis.quality_score;
            best_rgb = Some(rgb24);
            best_ts = actual_ts;
            best_attempt_idx = attempt;
        }

        let next_ts = ts + step;
        if next_ts > max_ts + 1e-3 {
            break;
        }
        ts = next_ts;
    }

    Ok(CapturedFrame {
        index: cell_index,
        timestamp_secs: best_ts,
        rgb24: best_rgb.unwrap_or_else(|| vec![0; (thumb_w * thumb_h * 3) as usize]),
        shifted_from_blank: best_attempt_idx > 0,
    })
}

fn extract_raw_rgb24(
    video_path: &Path,
    video_stream_index: u32,
    ts_secs: f64,
    thumb_w: u32,
    thumb_h: u32,
) -> Result<(Vec<u8>, f64)> {
    let expected = (thumb_w as usize) * (thumb_h as usize) * 3;
    let candidates = [ts_secs.max(0.0), (ts_secs - 1.0).max(0.0), 0.0];

    let mut last_err = String::new();
    for (attempt_idx, &candidate_ts) in candidates.iter().enumerate() {
        // Avoid duplicate identical retries at 0.0
        if attempt_idx > 0 && (candidate_ts - candidates[attempt_idx - 1]).abs() < 1e-3 {
            continue;
        }

        match run_ffmpeg_pipe(video_path, video_stream_index, candidate_ts, thumb_w, thumb_h) {
            Ok(mut bytes) => {
                if bytes.len() >= expected {
                    bytes.truncate(expected);
                    return Ok((bytes, candidate_ts));
                }
                last_err = format!(
                    "short frame buffer at {:.3}s ({} < {} bytes)",
                    candidate_ts,
                    bytes.len(),
                    expected
                );
            }
            Err(err) => {
                last_err = format!("{:#}", err);
            }
        }
    }

    bail!(
        "Failed extracting frame from {} around {:.3}s: {}",
        video_path.display(),
        ts_secs,
        last_err
    )
}

fn run_ffmpeg_pipe(
    video_path: &Path,
    video_stream_index: u32,
    ts_secs: f64,
    thumb_w: u32,
    thumb_h: u32,
) -> Result<Vec<u8>> {
    let ts_arg = format!("{:.3}", ts_secs);
    let map_arg = format!("0:{}", video_stream_index);
    let vf_arg = format!("scale={}:{}:flags=lanczos", thumb_w, thumb_h);

    let output = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-threads",
            "2",
            "-ss",
            &ts_arg,
            "-i",
        ])
        .arg(video_path)
        .args([
            "-map",
            &map_arg,
            "-frames:v",
            "1",
            "-vf",
            &vf_arg,
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "pipe:1",
        ])
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("Failed spawning ffmpeg for {}", video_path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ffmpeg exited with {}: {}", output.status, stderr.trim());
    }

    Ok(output.stdout)
}

pub fn analyze_frame(
    rgb24: &[u8],
    width: u32,
    height: u32,
    black_threshold: f32,
    white_threshold: f32,
) -> FrameAnalysis {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 || rgb24.len() < w * h * 3 {
        return FrameAnalysis {
            is_blank: true,
            blank_kind: Some(BlankKind::Solid),
            quality_score: 0.0,
        };
    }

    // Analyze central 70% x 70% region so 2.39:1 letterbox or 4:3 pillarbox black bars
    // do not distort blank-frame detection.
    let x_start = (w * 15) / 100;
    let x_end = ((w * 85) / 100).max(x_start + 1);
    let y_start = (h * 15) / 100;
    let y_end = ((h * 85) / 100).max(y_start + 1);

    let mut count = 0usize;
    let mut sum_y = 0.0f64;
    let mut sum_y2 = 0.0f64;
    let mut dark_pixels = 0usize;
    let mut bright_pixels = 0usize;

    let dark_cutoff = black_threshold + 4.0;
    let bright_cutoff = white_threshold - 4.0;

    for y in y_start..y_end {
        let row_base = y * w * 3;
        for x in x_start..x_end {
            let idx = row_base + x * 3;
            let r = rgb24[idx] as f32;
            let g = rgb24[idx + 1] as f32;
            let b = rgb24[idx + 2] as f32;
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;

            let ld = luma as f64;
            sum_y += ld;
            sum_y2 += ld * ld;
            if luma <= dark_cutoff {
                dark_pixels += 1;
            }
            if luma >= bright_cutoff {
                bright_pixels += 1;
            }
            count += 1;
        }
    }

    let n = count.max(1) as f64;
    let mean_y = (sum_y / n) as f32;
    let var_y = ((sum_y2 / n) - (sum_y / n).powi(2)).max(0.0);
    let std_y = var_y.sqrt() as f32;
    let dark_ratio = dark_pixels as f32 / count.max(1) as f32;
    let bright_ratio = bright_pixels as f32 / count.max(1) as f32;

    let is_black = mean_y < black_threshold || (dark_ratio >= 0.90 && std_y < 18.0);
    let is_white = mean_y > white_threshold || (bright_ratio >= 0.90 && std_y < 18.0);
    let is_solid = std_y < 3.5;

    let (is_blank, blank_kind) = if is_black {
        (true, Some(BlankKind::Black))
    } else if is_white {
        (true, Some(BlankKind::White))
    } else if is_solid {
        (true, Some(BlankKind::Solid))
    } else {
        (false, None)
    };

    let quality_score = std_y * 1.5 + (127.5 - (mean_y - 127.5).abs()) * 0.4;

    FrameAnalysis {
        is_blank,
        blank_kind,
        quality_score,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_solid_frame(w: u32, h: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
        let mut buf = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..(w * h) {
            buf.push(r);
            buf.push(g);
            buf.push(b);
        }
        buf
    }

    #[test]
    fn detects_black_white_and_solid_frames() {
        let black = make_solid_frame(100, 60, 2, 2, 2);
        let a_black = analyze_frame(&black, 100, 60, 18.0, 238.0);
        assert!(a_black.is_blank);
        assert_eq!(a_black.blank_kind, Some(BlankKind::Black));

        let white = make_solid_frame(100, 60, 250, 250, 250);
        let a_white = analyze_frame(&white, 100, 60, 18.0, 238.0);
        assert!(a_white.is_blank);
        assert_eq!(a_white.blank_kind, Some(BlankKind::White));

        let gray = make_solid_frame(100, 60, 128, 128, 128);
        let a_gray = analyze_frame(&gray, 100, 60, 18.0, 238.0);
        assert!(a_gray.is_blank);
        assert_eq!(a_gray.blank_kind, Some(BlankKind::Solid));
    }

    #[test]
    fn letterbox_bars_do_not_trigger_false_black() {
        let w = 100u32;
        let h = 100u32;
        let mut buf = make_solid_frame(w, h, 0, 0, 0);
        // Paint colorful textured region in center 50% (rows 25..75, cols 10..90)
        for y in 25..75 {
            for x in 10..90 {
                let idx = ((y * w + x) * 3) as usize;
                buf[idx] = ((x * 3) % 255) as u8;
                buf[idx + 1] = ((y * 4) % 255) as u8;
                buf[idx + 2] = 180;
            }
        }
        let analysis = analyze_frame(&buf, w, h, 18.0, 238.0);
        assert!(!analysis.is_blank);
        assert!(analysis.quality_score > 40.0);
    }

    #[test]
    fn textured_dark_scene_has_higher_quality_than_flat_black() {
        let w = 100u32;
        let h = 60u32;
        let flat = make_solid_frame(w, h, 4, 4, 4);
        let mut textured = make_solid_frame(w, h, 5, 5, 5);
        for y in 15..45 {
            for x in 20..80 {
                let idx = ((y * w + x) * 3) as usize;
                if (x + y) % 2 == 0 {
                    textured[idx] = 85;
                    textured[idx + 1] = 110;
                    textured[idx + 2] = 140;
                }
            }
        }
        let a_flat = analyze_frame(&flat, w, h, 18.0, 238.0);
        let a_tex = analyze_frame(&textured, w, h, 18.0, 238.0);
        assert!(!a_tex.is_blank);
        assert!(a_tex.quality_score > a_flat.quality_score);
    }
}
