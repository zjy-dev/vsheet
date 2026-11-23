mod cli;
mod extractor;
mod probe;
mod renderer;
mod scanner;

use anyhow::{bail, Result};
use clap::Parser;
use cli::{CliArgs, Mode, Theme};
use console::style;
use extractor::{extract_frames, SmartFrameConfig};
use indicatif::{ProgressBar, ProgressStyle};
use probe::{ensure_ffmpeg_tools, probe_video};
use rayon::prelude::*;
use renderer::{compute_geometry, init_font_database, render_and_save};
use scanner::{scan_videos, VideoJob};
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Instant;

fn main() -> ExitCode {
    match run() {
        Ok(had_failures) => {
            if had_failures {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(err) => {
            eprintln!("{} {:#}", style("Error:").red().bold(), err);
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let start = Instant::now();
    let args = CliArgs::parse();

    if args.list_themes {
        println!("{} Available color themes:", style("vsheet").cyan().bold());
        for theme in Theme::all() {
            println!("  {:8} {}", style(theme.name()).green().bold(), theme.description());
        }
        return Ok(false);
    }

    let grid = args
        .effective_grid()
        .map_err(|msg| anyhow::anyhow!(msg))?;
    let mode = args.effective_mode();
    let jobs = args.effective_jobs();
    let recursive = args.is_recursive();

    if args.black_threshold >= args.white_threshold {
        bail!(
            "--black-threshold ({}) must be lower than --white-threshold ({})",
            args.black_threshold,
            args.white_threshold
        );
    }

    rayon::ThreadPoolBuilder::new()
        .num_threads(jobs)
        .build_global()
        .ok();

    let scan = scan_videos(
        &args.paths,
        args.format,
        mode,
        recursive,
        args.max_depth,
        args.hidden,
        args.output.as_deref(),
    )?;

    for warn in &scan.input_warnings {
        eprintln!("{} {}", style("Warning:").yellow().bold(), warn);
    }
    for warn in &scan.collision_warnings {
        eprintln!("{} {}", style("Warning:").yellow().bold(), warn);
    }

    if args.dry_run {
        println!(
            "{} Dry-run scan: {} to generate, {} skipped (mode: {:?}, grid: {}, format: {})",
            style("vsheet").cyan().bold(),
            style(scan.jobs.len()).green().bold(),
            style(scan.skipped_existing.len()).dim(),
            mode,
            grid,
            args.format.ext(),
        );
        for job in &scan.jobs {
            println!(
                "  {} {} -> {}",
                style("[GEN]").green(),
                job.video_path.display(),
                job.output_path.display()
            );
        }
        for skipped in &scan.skipped_existing {
            println!("  {} {}", style("[SKIP]").dim(), skipped.display());
        }
        return Ok(false);
    }

    if scan.total_discovered() == 0 {
        println!("{} No matching video files found.", style("vsheet").cyan().bold());
        return Ok(false);
    }

    if scan.jobs.is_empty() {
        println!(
            "{} All {} video(s) already have contact sheets (skipped in {:.2}s). Use {} / {} to regenerate.",
            style("vsheet").cyan().bold(),
            style(scan.skipped_existing.len()).green().bold(),
            start.elapsed().as_secs_f64(),
            style("--overwrite").yellow(),
            style("--mode overwrite").yellow(),
        );
        return Ok(false);
    }

    ensure_ffmpeg_tools()?;
    let fontdb = init_font_database(args.font.as_deref());

    let smart_cfg = SmartFrameConfig {
        enabled: args.smart_frame_enabled(),
        blank_step: args.blank_step.as_option(),
        blank_retries: args.blank_retries,
        black_threshold: args.black_threshold,
        white_threshold: args.white_threshold,
    };

    let mode_label = match mode {
        Mode::Skip => "skip",
        Mode::Overwrite => "overwrite",
    };

    println!(
        "{} Processing {} video(s) ({} skipped, mode={}, grid={}, theme={}, recursive={}, threads={})",
        style("vsheet").cyan().bold(),
        style(scan.jobs.len()).bold(),
        scan.skipped_existing.len(),
        mode_label,
        grid,
        args.theme.name(),
        recursive,
        jobs,
    );

    let pb = ProgressBar::new(scan.jobs.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.cyan} [{elapsed_precise}] [{bar:32.cyan/blue}] {pos}/{len} {msg}",
        )
        .unwrap()
        .progress_chars("=> "),
    );

    let generated = AtomicUsize::new(0);
    let failed = AtomicUsize::new(0);
    let shifted_total = AtomicUsize::new(0);
    let errors: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let shift_notes: Mutex<Vec<String>> = Mutex::new(Vec::new());

    let parallel_cells = scan.jobs.len() < jobs;

    scan.jobs.par_iter().for_each(|job: &VideoJob| {
        let res = (|| -> Result<usize> {
            let meta = probe_video(&job.video_path)?;
            let geom = compute_geometry(args.width, args.padding, args.gap, grid, &meta);
            let extracted = extract_frames(
                &job.video_path,
                &meta,
                grid,
                geom.cell_w,
                geom.cell_h,
                &smart_cfg,
                parallel_cells,
            )?;
            render_and_save(
                &job.output_path,
                args.format,
                args.quality,
                args.theme,
                &geom,
                &meta,
                &extracted.frames,
                args.show_timestamps(),
                &fontdb,
            )?;
            Ok(extracted.shifted_count)
        })();

        match res {
            Ok(shifted) => {
                generated.fetch_add(1, Ordering::Relaxed);
                if shifted > 0 {
                    shifted_total.fetch_add(shifted, Ordering::Relaxed);
                    if let Ok(mut notes) = shift_notes.lock() {
                        notes.push(format!(
                            "  {} {} -> {} ({} blank cell(s) forward-shifted)",
                            style("↳").cyan(),
                            job.video_path
                                .file_name()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            job.output_path
                                .file_name()
                                .map(|s| s.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                            shifted
                        ));
                    }
                }
            }
            Err(err) => {
                failed.fetch_add(1, Ordering::Relaxed);
                if let Ok(mut errs) = errors.lock() {
                    errs.push(format!("{}: {:#}", job.video_path.display(), err));
                }
            }
        }
        pb.inc(1);
    });

    pb.finish_and_clear();

    let ok_count = generated.load(Ordering::Relaxed);
    let fail_count = failed.load(Ordering::Relaxed);
    let shifted_count = shifted_total.load(Ordering::Relaxed);
    let elapsed = start.elapsed().as_secs_f64();

    if let Ok(notes) = shift_notes.lock() {
        for note in notes.iter() {
            println!("{}", note);
        }
    }

    if let Ok(errs) = errors.lock() {
        for err in errs.iter() {
            eprintln!("{} {}", style("Failed:").red().bold(), err);
        }
    }

    println!(
        "{} Done in {:.2}s — {} generated, {} skipped, {} failed{}",
        style("vsheet").green().bold(),
        elapsed,
        style(ok_count).green().bold(),
        style(scan.skipped_existing.len()).dim(),
        if fail_count > 0 {
            style(fail_count).red().bold()
        } else {
            style(fail_count).dim()
        },
        if shifted_count > 0 {
            format!(", {} blank frame(s) auto-shifted", shifted_count)
        } else {
            String::new()
        }
    );

    Ok(fail_count > 0)
}
