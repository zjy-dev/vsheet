use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct VideoMetadata {
    pub file_name: String,
    pub file_size_bytes: u64,
    pub duration_secs: f64,
    pub video_stream_index: u32,
    pub video_codec: String,
    pub display_width: u32,
    pub display_height: u32,
    pub fps: Option<f64>,
    pub bit_depth: Option<u8>,
    pub hdr_label: Option<String>,
    pub bitrate_bps: Option<u64>,
    pub audio_codec: Option<String>,
    pub audio_sample_rate_hz: Option<u32>,
    pub audio_channels_label: Option<String>,
}

impl VideoMetadata {
    pub fn aspect_ratio_label(&self) -> String {
        format_aspect_ratio(self.display_width, self.display_height)
    }
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    #[serde(default)]
    streams: Vec<FfprobeStream>,
    format: Option<FfprobeFormat>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    size: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    index: Option<u32>,
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    sample_aspect_ratio: Option<String>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    duration: Option<String>,
    bit_rate: Option<String>,
    bits_per_raw_sample: Option<String>,
    pix_fmt: Option<String>,
    color_transfer: Option<String>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    disposition: Option<FfprobeDisposition>,
    #[serde(default)]
    tags: HashMap<String, String>,
    #[serde(default)]
    side_data_list: Vec<FfprobeSideData>,
}

#[derive(Debug, Deserialize)]
struct FfprobeDisposition {
    attached_pic: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct FfprobeSideData {
    side_data_type: Option<String>,
    rotation: Option<i32>,
}

pub fn ensure_ffmpeg_tools() -> Result<()> {
    for bin in ["ffprobe", "ffmpeg"] {
        let output = Command::new(bin).arg("-version").output().with_context(|| {
            format!(
                "Failed to execute '{}'. Please install FFmpeg and ensure '{}' is in PATH.",
                bin, bin
            )
        })?;
        if !output.status.success() {
            bail!(
                "'{} -version' exited with status {}. Please verify your FFmpeg installation.",
                bin,
                output.status
            );
        }
    }
    Ok(())
}

pub fn probe_video(path: &Path) -> Result<VideoMetadata> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .output()
        .with_context(|| format!("Failed to launch ffprobe on {}", path.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "ffprobe failed on {} (status {}): {}",
            path.display(),
            output.status,
            stderr.trim()
        );
    }

    let parsed: FfprobeOutput = serde_json::from_slice(&output.stdout)
        .with_context(|| format!("Failed to parse ffprobe JSON for {}", path.display()))?;

    let file_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    let fs_size = fs::metadata(path).map(|m| m.len()).ok();
    let format_size = parsed
        .format
        .as_ref()
        .and_then(|f| f.size.as_deref())
        .and_then(|s| s.parse::<u64>().ok());
    let file_size_bytes = fs_size.or(format_size).unwrap_or(0);

    let video_stream = parsed
        .streams
        .iter()
        .find(|s| {
            s.codec_type.as_deref() == Some("video")
                && s.disposition.as_ref().and_then(|d| d.attached_pic) != Some(1)
        })
        .ok_or_else(|| anyhow!("No main video stream found in {}", path.display()))?;

    let video_stream_index = video_stream.index.unwrap_or(0);

    let format_duration = parsed
        .format
        .as_ref()
        .and_then(|f| f.duration.as_deref())
        .and_then(parse_duration_str);
    let stream_duration = video_stream
        .duration
        .as_deref()
        .and_then(parse_duration_str);
    let tag_duration = find_tag_duration(&video_stream.tags);
    let duration_secs = format_duration
        .or(stream_duration)
        .or(tag_duration)
        .unwrap_or(0.0)
        .max(0.0);

    let coded_width = video_stream.width.unwrap_or(0);
    let coded_height = video_stream.height.unwrap_or(0);
    if coded_width == 0 || coded_height == 0 {
        bail!("Video stream reports invalid dimensions 0x0 in {}", path.display());
    }

    let rotation = extract_rotation(video_stream);
    let (display_width, display_height) = compute_display_dimensions(
        coded_width,
        coded_height,
        video_stream.sample_aspect_ratio.as_deref(),
        rotation,
    );

    let video_codec = format_video_codec(video_stream.codec_name.as_deref());

    let fps = video_stream
        .avg_frame_rate
        .as_deref()
        .and_then(parse_rational_fps)
        .or_else(|| {
            video_stream
                .r_frame_rate
                .as_deref()
                .and_then(parse_rational_fps)
        });

    let bit_depth = infer_bit_depth(
        video_stream.bits_per_raw_sample.as_deref(),
        video_stream.pix_fmt.as_deref(),
    );

    let (_is_hdr, hdr_label) = detect_hdr(video_stream);

    let format_bitrate = parsed
        .format
        .as_ref()
        .and_then(|f| f.bit_rate.as_deref())
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&b| b > 0);
    let stream_bitrate = video_stream
        .bit_rate
        .as_deref()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|&b| b > 0);
    let computed_bitrate = if duration_secs > 0.1 && file_size_bytes > 0 {
        Some(((file_size_bytes as f64 * 8.0) / duration_secs).round() as u64)
    } else {
        None
    };
    let bitrate_bps = format_bitrate.or(stream_bitrate).or(computed_bitrate);

    let audio_stream = parsed
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"));

    let audio_codec = audio_stream.map(|a| format_audio_codec(a.codec_name.as_deref()));
    let audio_sample_rate_hz = audio_stream
        .and_then(|a| a.sample_rate.as_deref())
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|&sr| sr > 0);
    let audio_channels_label = audio_stream
        .and_then(|a| a.channels)
        .filter(|&c| c > 0)
        .map(format_channels);

    Ok(VideoMetadata {
        file_name,
        file_size_bytes,
        duration_secs,
        video_stream_index,
        video_codec,
        display_width,
        display_height,
        fps,
        bit_depth,
        hdr_label,
        bitrate_bps,
        audio_codec,
        audio_sample_rate_hz,
        audio_channels_label,
    })
}

fn find_tag_duration(tags: &HashMap<String, String>) -> Option<f64> {
    if let Some(v) = tags.get("DURATION").or_else(|| tags.get("DURATION-eng")) {
        if let Some(secs) = parse_duration_str(v) {
            return Some(secs);
        }
    }
    for (k, v) in tags {
        if k.to_ascii_uppercase().starts_with("DURATION") {
            if let Some(secs) = parse_duration_str(v) {
                return Some(secs);
            }
        }
    }
    None
}

pub fn parse_duration_str(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("N/A") {
        return None;
    }
    if let Ok(val) = trimmed.parse::<f64>() {
        if val.is_finite() && val >= 0.0 {
            return Some(val);
        }
        return None;
    }
    // Parse HH:MM:SS.xxx or MM:SS.xxx
    let parts: Vec<&str> = trimmed.split(':').collect();
    match parts.len() {
        3 => {
            let h: f64 = parts[0].parse().ok()?;
            let m: f64 = parts[1].parse().ok()?;
            let sec: f64 = parts[2].parse().ok()?;
            let total = h * 3600.0 + m * 60.0 + sec;
            if total.is_finite() && total >= 0.0 {
                Some(total)
            } else {
                None
            }
        }
        2 => {
            let m: f64 = parts[0].parse().ok()?;
            let sec: f64 = parts[1].parse().ok()?;
            let total = m * 60.0 + sec;
            if total.is_finite() && total >= 0.0 {
                Some(total)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn extract_rotation(stream: &FfprobeStream) -> i32 {
    for sd in &stream.side_data_list {
        if let Some(rot) = sd.rotation {
            return rot;
        }
    }
    if let Some(rot_str) = stream.tags.get("rotate") {
        if let Ok(rot) = rot_str.trim().parse::<i32>() {
            return rot;
        }
    }
    0
}

pub fn compute_display_dimensions(
    coded_w: u32,
    coded_h: u32,
    sar_str: Option<&str>,
    rotation: i32,
) -> (u32, u32) {
    let mut w = coded_w as f64;
    let h = coded_h as f64;

    if let Some(sar) = sar_str.and_then(parse_ratio) {
        if sar > 0.0 && (sar - 1.0).abs() > 0.01 {
            w = (w * sar).round();
        }
    }

    let mut dw = (w.round() as u32).max(1);
    let mut dh = (h.round() as u32).max(1);

    let norm_rot = ((rotation % 360) + 360) % 360;
    if norm_rot == 90 || norm_rot == 270 {
        std::mem::swap(&mut dw, &mut dh);
    }

    (dw, dh)
}

fn parse_ratio(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("N/A") {
        return None;
    }
    let sep = if trimmed.contains(':') { ':' } else { '/' };
    let (num_s, den_s) = trimmed.split_once(sep)?;
    let num: f64 = num_s.trim().parse().ok()?;
    let den: f64 = den_s.trim().parse().ok()?;
    if num > 0.0 && den > 0.0 {
        Some(num / den)
    } else {
        None
    }
}

fn parse_rational_fps(s: &str) -> Option<f64> {
    let trimmed = s.trim();
    if trimmed.is_empty() || trimmed == "0/0" || trimmed.eq_ignore_ascii_case("N/A") {
        return None;
    }
    let fps = if let Some((num_s, den_s)) = trimmed.split_once('/') {
        let num: f64 = num_s.trim().parse().ok()?;
        let den: f64 = den_s.trim().parse().ok()?;
        if den <= 0.0 {
            return None;
        }
        num / den
    } else {
        trimmed.parse::<f64>().ok()?
    };
    if fps.is_finite() && fps > 0.0 && fps <= 1000.0 {
        Some(fps)
    } else {
        None
    }
}

fn format_video_codec(raw: Option<&str>) -> String {
    match raw.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("h264") => "H.264".to_string(),
        Some("hevc") | Some("h265") => "HEVC".to_string(),
        Some("av1") => "AV1".to_string(),
        Some("vp9") => "VP9".to_string(),
        Some("vp8") => "VP8".to_string(),
        Some("mpeg4") => "MPEG-4".to_string(),
        Some("mpeg2video") => "MPEG-2".to_string(),
        Some("mpeg1video") => "MPEG-1".to_string(),
        Some("prores") => "ProRes".to_string(),
        Some("vc1") => "VC-1".to_string(),
        Some("wmv3") => "WMV3".to_string(),
        Some(other) => other.to_ascii_uppercase(),
        None => "UNKNOWN".to_string(),
    }
}

fn format_audio_codec(raw: Option<&str>) -> String {
    match raw.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("aac") => "AAC".to_string(),
        Some("eac3") => "EAC3".to_string(),
        Some("ac3") => "AC3".to_string(),
        Some("flac") => "FLAC".to_string(),
        Some("opus") => "Opus".to_string(),
        Some("mp3") => "MP3".to_string(),
        Some("dts") => "DTS".to_string(),
        Some("truehd") => "TrueHD".to_string(),
        Some("vorbis") => "Vorbis".to_string(),
        Some("alac") => "ALAC".to_string(),
        Some(s) if s.starts_with("pcm_") => "PCM".to_string(),
        Some(other) => other.to_ascii_uppercase(),
        None => "Audio".to_string(),
    }
}

fn infer_bit_depth(bits_per_raw: Option<&str>, pix_fmt: Option<&str>) -> Option<u8> {
    if let Some(b) = bits_per_raw.and_then(|s| s.trim().parse::<u8>().ok()) {
        if b >= 8 {
            return Some(b);
        }
    }
    if let Some(fmt) = pix_fmt.map(|s| s.to_ascii_lowercase()) {
        if fmt.contains("16le") || fmt.contains("16be") || fmt.contains("p016") {
            return Some(16);
        }
        if fmt.contains("12le") || fmt.contains("12be") || fmt.contains("p012") || fmt.contains("p12") {
            return Some(12);
        }
        if fmt.contains("10le") || fmt.contains("10be") || fmt.contains("p010") || fmt.contains("p10") {
            return Some(10);
        }
        if !fmt.is_empty() {
            return Some(8);
        }
    }
    None
}

fn detect_hdr(stream: &FfprobeStream) -> (bool, Option<String>) {
    for sd in &stream.side_data_list {
        if let Some(kind) = sd.side_data_type.as_deref() {
            let lower = kind.to_ascii_lowercase();
            if lower.contains("dolby") || lower.contains("dovi") {
                return (true, Some("Dolby Vision".to_string()));
            }
        }
    }
    match stream.color_transfer.as_deref() {
        Some("smpte2084") => return (true, Some("HDR10".to_string())),
        Some("arib-std-b67") => return (true, Some("HLG".to_string())),
        _ => {}
    }
    for sd in &stream.side_data_list {
        if let Some(kind) = sd.side_data_type.as_deref() {
            let lower = kind.to_ascii_lowercase();
            if lower.contains("mastering display") {
                return (true, Some("HDR".to_string()));
            }
        }
    }
    (false, None)
}

fn format_channels(ch: u32) -> String {
    match ch {
        1 => "Mono".to_string(),
        2 => "Stereo".to_string(),
        6 => "5.1ch".to_string(),
        8 => "7.1ch".to_string(),
        n => format!("{}ch", n),
    }
}

pub fn format_duration(secs: f64) -> String {
    let total = secs.max(0.0).round() as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_bitrate(bps: u64) -> String {
    if bps >= 1_000_000 {
        format!("{:.1} Mbps", bps as f64 / 1_000_000.0)
    } else if bps >= 1_000 {
        format!("{:.0} kbps", bps as f64 / 1_000.0)
    } else {
        format!("{} bps", bps)
    }
}

pub fn format_aspect_ratio(w: u32, h: u32) -> String {
    if w == 0 || h == 0 {
        return "-".to_string();
    }
    let ratio = w as f64 / h as f64;
    let common: &[(&str, f64)] = &[
        ("16:9", 16.0 / 9.0),
        ("9:16", 9.0 / 16.0),
        ("4:3", 4.0 / 3.0),
        ("3:4", 3.0 / 4.0),
        ("21:9", 21.0 / 9.0),
        ("16:10", 16.0 / 10.0),
        ("1:1", 1.0),
        ("2.39:1", 2.39),
        ("2.35:1", 2.35),
        ("1.85:1", 1.85),
    ];
    for &(label, target) in common {
        if (ratio - target).abs() < 0.03 {
            return label.to_string();
        }
    }
    let g = gcd(w, h);
    let rw = w / g;
    let rh = h / g;
    if rw <= 32 && rh <= 32 {
        format!("{}:{}", rw, rh)
    } else {
        format!("{:.2}:1", ratio)
    }
}

fn gcd(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_various_duration_formats() {
        assert!((parse_duration_str("123.45").unwrap() - 123.45).abs() < 1e-6);
        assert!((parse_duration_str("01:02:03.500000").unwrap() - 3723.5).abs() < 1e-6);
        assert!((parse_duration_str("05:30").unwrap() - 330.0).abs() < 1e-6);
        assert!(parse_duration_str("N/A").is_none());
    }

    #[test]
    fn computes_display_dimensions_with_sar_and_rotation() {
        // Anamorphic 1440x1080 with 4:3 SAR -> 1920x1080
        assert_eq!(
            compute_display_dimensions(1440, 1080, Some("4:3"), 0),
            (1920, 1080)
        );
        // Portrait phone video 1920x1080 rotated 90 degrees -> 1080x1920
        assert_eq!(
            compute_display_dimensions(1920, 1080, Some("1:1"), 90),
            (1080, 1920)
        );
        // -90 rotation
        assert_eq!(
            compute_display_dimensions(1920, 1080, None, -90),
            (1080, 1920)
        );
    }

    #[test]
    fn formats_size_duration_and_bitrate() {
        assert_eq!(format_duration(5076.0), "01:24:36");
        assert_eq!(format_file_size(3_840_000_000), "3.58 GB");
        assert_eq!(format_bitrate(14_200_000), "14.2 Mbps");
        assert_eq!(format_aspect_ratio(1920, 1080), "16:9");
        assert_eq!(format_aspect_ratio(1080, 1920), "9:16");
    }
}
