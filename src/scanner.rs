use crate::cli::{Mode, OutputFormat};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::{DirEntry, WalkDir};

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "mov", "avi", "webm", "flv", "wmv", "m4v", "ts", "m2ts", "mts", "mpg", "mpeg",
    "3gp", "rmvb", "vob", "ogv",
];

const IGNORED_IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "gif"];

#[derive(Debug, Clone)]
pub struct VideoJob {
    pub video_path: PathBuf,
    pub output_path: PathBuf,
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub jobs: Vec<VideoJob>,
    pub skipped_existing: Vec<PathBuf>,
    pub collision_warnings: Vec<String>,
    pub input_warnings: Vec<String>,
}

impl ScanReport {
    pub fn total_discovered(&self) -> usize {
        self.jobs.len() + self.skipped_existing.len()
    }
}

pub fn is_video_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            VIDEO_EXTENSIONS.iter().any(|&v| v == lower)
        })
        .unwrap_or(false)
}

fn is_ignored_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let lower = ext.to_ascii_lowercase();
            IGNORED_IMAGE_EXTENSIONS.iter().any(|&v| v == lower)
        })
        .unwrap_or(false)
}

pub fn target_output_path(video_path: &Path, format: OutputFormat) -> PathBuf {
    video_path.with_extension(format.ext())
}

fn has_sheet_image_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png"))
        .unwrap_or(false)
}

fn resolve_job_output_path(
    video_path: &Path,
    format: OutputFormat,
    output: Option<&Path>,
    single_explicit_file: bool,
) -> PathBuf {
    match output {
        None => target_output_path(video_path, format),
        Some(out) => {
            if single_explicit_file && !out.is_dir() && has_sheet_image_extension(out) {
                out.to_path_buf()
            } else {
                let name = video_path
                    .file_name()
                    .map(|f| Path::new(f).with_extension(format.ext()))
                    .unwrap_or_else(|| PathBuf::from(format!("sheet.{}", format.ext())));
                out.join(name)
            }
        }
    }
}

fn is_not_hidden(entry: &DirEntry, include_hidden: bool) -> bool {
    if include_hidden || entry.depth() == 0 {
        return true;
    }
    entry
        .file_name()
        .to_str()
        .map(|name| !name.starts_with('.'))
        .unwrap_or(true)
}

pub fn scan_videos(
    inputs: &[PathBuf],
    format: OutputFormat,
    mode: Mode,
    recursive: bool,
    max_depth: Option<usize>,
    include_hidden: bool,
    output: Option<&Path>,
) -> Result<ScanReport> {
    let mut discovered: Vec<PathBuf> = Vec::new();
    let mut input_warnings: Vec<String> = Vec::new();

    for input in inputs {
        if !input.exists() {
            input_warnings.push(format!("Path does not exist: {}", input.display()));
            continue;
        }

        if input.is_file() {
            if is_ignored_image_extension(input) {
                input_warnings.push(format!(
                    "Skipping static image file '{}'",
                    input.display()
                ));
            } else {
                discovered.push(input.clone());
            }
            continue;
        }

        if input.is_dir() {
            let mut walker = WalkDir::new(input).follow_links(false);
            if !recursive {
                walker = walker.max_depth(1);
            } else if let Some(depth) = max_depth {
                walker = walker.max_depth(depth);
            }

            for entry_res in walker.into_iter().filter_entry(|e| is_not_hidden(e, include_hidden)) {
                match entry_res {
                    Ok(entry) => {
                        if entry.file_type().is_file() && is_video_extension(entry.path()) {
                            discovered.push(entry.into_path());
                        }
                    }
                    Err(err) => {
                        input_warnings.push(format!("Failed reading entry: {}", err));
                    }
                }
            }
        } else {
            input_warnings.push(format!(
                "Skipping unsupported path type: {}",
                input.display()
            ));
        }
    }

    // Deduplicate identical input paths and sort deterministically.
    discovered.sort();
    discovered.dedup();

    let mut report = ScanReport {
        input_warnings,
        ..Default::default()
    };

    let single_file = discovered.len() == 1;
    let mut seen_outputs: HashSet<PathBuf> = HashSet::new();

    for video_path in discovered {
        let output_path =
            resolve_job_output_path(&video_path, format, output, single_file);

        // Guard against overwriting the input file itself if someone passed an image file directly.
        if output_path == video_path {
            report.input_warnings.push(format!(
                "Skipping '{}' because target path equals input file",
                video_path.display()
            ));
            continue;
        }

        // Canonicalize parent directory for collision detection when possible so `./a.mp4` and `a.mp4`
        // or `a.mp4` and `a.mkv` in the same folder are detected reliably.
        let collision_key = normalize_collision_key(&output_path)
            .with_context(|| format!("failed resolving path {}", output_path.display()))?;

        if !seen_outputs.insert(collision_key) {
            report.collision_warnings.push(format!(
                "Name collision: '{}' skipped because '{}' is already targeted by another video",
                video_path.display(),
                output_path.display()
            ));
            continue;
        }

        if mode == Mode::Skip && existing_non_empty_file(&output_path) {
            report.skipped_existing.push(video_path);
            continue;
        }

        report.jobs.push(VideoJob {
            video_path,
            output_path,
        });
    }

    Ok(report)
}

fn normalize_collision_key(output_path: &Path) -> Result<PathBuf> {
    let parent = match output_path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let abs_parent = fs::canonicalize(parent).unwrap_or_else(|_| parent.to_path_buf());
    let file_name = output_path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    Ok(abs_parent.join(file_name))
}

fn existing_non_empty_file(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.len() > 0,
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("vsheet_test_{}_{}", label, nanos));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn video_extension_detection_is_case_insensitive() {
        assert!(is_video_extension(Path::new("movie.MP4")));
        assert!(is_video_extension(Path::new("clip.MkV")));
        assert!(is_video_extension(Path::new("stream.m2ts")));
        assert!(!is_video_extension(Path::new("cover.jpg")));
        assert!(!is_video_extension(Path::new("notes.txt")));
    }

    #[test]
    fn target_output_replaces_extension_only() {
        let out = target_output_path(Path::new("/data/videos/样片.v1.mp4"), OutputFormat::Jpg);
        assert_eq!(out, PathBuf::from("/data/videos/样片.v1.jpg"));
        let out_png = target_output_path(Path::new("/data/videos/样片.v1.mkv"), OutputFormat::Png);
        assert_eq!(out_png, PathBuf::from("/data/videos/样片.v1.png"));
    }

    #[test]
    fn scanner_handles_skip_overwrite_and_collisions() {
        let dir = make_temp_dir("scan");
        let sub = dir.join("nested");
        let hidden = dir.join(".hidden_dir");
        fs::create_dir_all(&sub).unwrap();
        fs::create_dir_all(&hidden).unwrap();

        let v1 = dir.join("alpha.mp4");
        let v1_dup = dir.join("alpha.mkv");
        let v2 = sub.join("beta.mov");
        let v_hidden = hidden.join("secret.mp4");
        fs::write(&v1, b"video1").unwrap();
        fs::write(&v1_dup, b"video1_dup").unwrap();
        fs::write(&v2, b"video2").unwrap();
        fs::write(&v_hidden, b"hidden").unwrap();

        // Pre-create beta.jpg as non-empty
        fs::write(sub.join("beta.jpg"), b"existing_jpg").unwrap();

        let skip_report = scan_videos(
            &[dir.clone()],
            OutputFormat::Jpg,
            Mode::Skip,
            true,
            None,
            false,
            None,
        )
        .unwrap();

        assert_eq!(skip_report.jobs.len(), 1);
        assert_eq!(skip_report.jobs[0].video_path, v1_dup);
        assert_eq!(skip_report.skipped_existing, vec![v2.clone()]);
        assert_eq!(skip_report.collision_warnings.len(), 1);

        let overwrite_report = scan_videos(
            &[dir.clone()],
            OutputFormat::Jpg,
            Mode::Overwrite,
            true,
            None,
            false,
            None,
        )
        .unwrap();
        assert_eq!(overwrite_report.jobs.len(), 2);
        assert!(overwrite_report.skipped_existing.is_empty());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn scanner_handles_single_file_and_non_recursive_mode() {
        let dir = make_temp_dir("rec");
        let sub = dir.join("nested");
        fs::create_dir_all(&sub).unwrap();

        let top_vid = dir.join("top.mp4");
        let nested_vid = sub.join("child.mp4");
        let img_file = dir.join("cover.jpg");
        fs::write(&top_vid, b"top").unwrap();
        fs::write(&nested_vid, b"nested").unwrap();
        fs::write(&img_file, b"img").unwrap();

        // Single file input + static image file warning
        let single_report = scan_videos(
            &[top_vid.clone(), img_file.clone()],
            OutputFormat::Jpg,
            Mode::Skip,
            true,
            None,
            false,
            None,
        )
        .unwrap();
        assert_eq!(single_report.jobs.len(), 1);
        assert_eq!(single_report.jobs[0].video_path, top_vid);
        assert_eq!(single_report.input_warnings.len(), 1);

        // Non-recursive directory scan should exclude nested/child.mp4
        let non_rec = scan_videos(
            &[dir.clone()],
            OutputFormat::Jpg,
            Mode::Skip,
            false,
            None,
            false,
            None,
        )
        .unwrap();
        assert_eq!(non_rec.jobs.len(), 1);
        assert_eq!(non_rec.jobs[0].video_path, top_vid);

        // Recursive directory scan should include both
        let rec = scan_videos(
            &[dir.clone()],
            OutputFormat::Jpg,
            Mode::Skip,
            true,
            None,
            false,
            None,
        )
        .unwrap();
        assert_eq!(rec.jobs.len(), 2);

        // Custom file output for single video
        let custom_file = dir.join("custom_out/sheet.jpg");
        let out_file_report = scan_videos(
            &[top_vid.clone()],
            OutputFormat::Jpg,
            Mode::Skip,
            true,
            None,
            false,
            Some(&custom_file),
        )
        .unwrap();
        assert_eq!(out_file_report.jobs[0].output_path, custom_file);

        // Custom directory output for batch
        let custom_dir = dir.join("sheets_dir");
        let out_dir_report = scan_videos(
            &[dir.clone()],
            OutputFormat::Jpg,
            Mode::Skip,
            true,
            None,
            false,
            Some(&custom_dir),
        )
        .unwrap();
        assert_eq!(
            out_dir_report.jobs[0].output_path,
            custom_dir.join("child.jpg")
        );
        assert_eq!(
            out_dir_report.jobs[1].output_path,
            custom_dir.join("top.jpg")
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
