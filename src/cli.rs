use clap::{Parser, ValueEnum};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    /// Skip videos whose target contact sheet already exists and is non-empty
    Skip,
    /// Regenerate and overwrite existing contact sheets
    Overwrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Jpg,
    Png,
}

impl OutputFormat {
    pub fn ext(self) -> &'static str {
        match self {
            OutputFormat::Jpg => "jpg",
            OutputFormat::Png => "png",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Theme {
    /// 暖杏晨光（默认：暖奶油底色 + 焦糖橙强调，温和耐看）
    #[value(alias = "warm")]
    Apricot,
    /// 极简画廊白（瑞士美术馆暖灰白 + 纯白画框，简洁纯粹）
    #[value(alias = "light", alias = "classic")]
    Gallery,
    /// 黑金奢雅（深海军蓝 + 香槟金点缀，高级杂志质感）
    Luxury,
    /// 森系鼠尾草（柔和灰绿 + 森林翠绿，自然护眼）
    #[value(alias = "forest")]
    Botanical,
    /// 深海青岚（深海蓝灰 + 松石青绿，沉静专业）
    #[value(alias = "slate")]
    Ocean,
    /// 落日陶土（暖沙底色 + 珊瑚赤陶，活力温暖）
    Terracotta,
    /// 沙漠玫瑰（柔粉灰调 + 勃艮第酒红，优雅别致）
    Rose,
    /// 极地冰蓝（冰蓝底色 + 钢蓝强调，清爽通透）
    #[value(alias = "nord")]
    Frost,
    /// 午夜星云（深紫夜空 + 薰衣草紫，氛围感暗色调）
    Galaxy,
    /// 曜石暗金（影院黑炭灰 + 琥珀金强调，沉浸看片）
    #[value(alias = "dark", alias = "oled")]
    Obsidian,
}

impl Theme {
    pub fn all() -> &'static [Theme] {
        &[
            Theme::Apricot,
            Theme::Gallery,
            Theme::Luxury,
            Theme::Botanical,
            Theme::Ocean,
            Theme::Terracotta,
            Theme::Rose,
            Theme::Frost,
            Theme::Galaxy,
            Theme::Obsidian,
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            Theme::Apricot => "apricot",
            Theme::Gallery => "gallery",
            Theme::Luxury => "luxury",
            Theme::Botanical => "botanical",
            Theme::Ocean => "ocean",
            Theme::Terracotta => "terracotta",
            Theme::Rose => "rose",
            Theme::Frost => "frost",
            Theme::Galaxy => "galaxy",
            Theme::Obsidian => "obsidian",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Theme::Apricot => "暖杏晨光 — 暖奶油画布 + 暖白顶栏 + 焦糖橙强调（默认，兼容别名: warm）",
            Theme::Gallery => "极简画廊白 — 瑞士暖灰白底 + 纯白画框 + 炭黑强调（兼容别名: light/classic）",
            Theme::Luxury => "黑金奢雅 — 深海军蓝 + 香槟金，高对比优雅杂志风格",
            Theme::Botanical => "森系鼠尾草 — 柔和草本灰绿 + 森林绿强调（兼容别名: forest）",
            Theme::Ocean => "深海青岚 — 深海藏蓝 + 青碧海蓝，专业沉稳暗色调（兼容别名: slate）",
            Theme::Terracotta => "落日陶土 — 暖沙底色 + 珊瑚橙 + 深青字色，明亮有活力",
            Theme::Rose => "沙漠玫瑰 — 柔粉暖灰 + 酒红文字，柔和高级文艺感",
            Theme::Frost => "极地冰蓝 — 清透冰蓝 + 钢蓝强调，清爽冷静（兼容别名: nord）",
            Theme::Galaxy => "午夜星云 — 深紫星夜 + 薰衣草紫，氛围感暗色调",
            Theme::Obsidian => "曜石暗金 — 影院炭黑 + 琥珀金，纯暗沉浸看片（兼容别名: dark/oled）",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlankStep {
    Auto,
    Fixed(f64),
}

impl BlankStep {
    pub fn as_option(self) -> Option<f64> {
        match self {
            BlankStep::Auto => None,
            BlankStep::Fixed(v) => Some(v),
        }
    }
}

impl fmt::Display for BlankStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BlankStep::Auto => f.write_str("auto"),
            BlankStep::Fixed(v) => write!(f, "{}", v),
        }
    }
}

impl FromStr for BlankStep {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.eq_ignore_ascii_case("auto") {
            return Ok(BlankStep::Auto);
        }
        let v: f64 = trimmed
            .parse()
            .map_err(|_| format!("invalid blank-step '{}', expected 'auto' or positive seconds", s))?;
        if v <= 0.0 || !v.is_finite() {
            return Err(format!("blank-step must be 'auto' or positive seconds, got {}", v));
        }
        Ok(BlankStep::Fixed(v))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSpec {
    pub cols: u32,
    pub rows: u32,
}

impl GridSpec {
    pub fn total_cells(self) -> u32 {
        self.cols * self.rows
    }

    pub fn validate(self) -> Result<Self, String> {
        if !(1..=12).contains(&self.cols) {
            return Err(format!(
                "grid columns must be between 1 and 12, got {}",
                self.cols
            ));
        }
        if !(1..=20).contains(&self.rows) {
            return Err(format!(
                "grid rows must be between 1 and 20, got {}",
                self.rows
            ));
        }
        Ok(self)
    }
}

impl fmt::Display for GridSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}x{}", self.cols, self.rows)
    }
}

impl FromStr for GridSpec {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.trim().to_ascii_lowercase();
        let (cols_str, rows_str) = lower
            .split_once('x')
            .ok_or_else(|| format!("invalid grid '{}', expected COLSxROWS like 4x4 or 3x3", s))?;
        let cols: u32 = cols_str
            .trim()
            .parse()
            .map_err(|_| format!("invalid column count in grid '{}'", s))?;
        let rows: u32 = rows_str
            .trim()
            .parse()
            .map_err(|_| format!("invalid row count in grid '{}'", s))?;
        GridSpec { cols, rows }.validate()
    }
}

fn default_jobs() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8)
}

fn parse_width(s: &str) -> Result<u32, String> {
    let w: u32 = s
        .parse()
        .map_err(|_| format!("invalid width '{}'", s))?;
    if w < 800 {
        return Err(format!("width must be at least 800 pixels, got {}", w));
    }
    Ok(w)
}

fn parse_quality(s: &str) -> Result<u8, String> {
    let q: u8 = s
        .parse()
        .map_err(|_| format!("invalid quality '{}'", s))?;
    if !(1..=100).contains(&q) {
        return Err(format!("JPEG quality must be between 1 and 100, got {}", q));
    }
    Ok(q)
}

fn parse_threshold(s: &str) -> Result<f32, String> {
    let v: f32 = s
        .parse()
        .map_err(|_| format!("invalid luminance threshold '{}'", s))?;
    if !(0.0..=255.0).contains(&v) || v.is_nan() {
        return Err(format!(
            "luminance threshold must be between 0.0 and 255.0, got {}",
            v
        ));
    }
    Ok(v)
}

fn parse_jobs(s: &str) -> Result<usize, String> {
    let j: usize = s
        .parse()
        .map_err(|_| format!("invalid jobs '{}'", s))?;
    if j == 0 {
        return Err("jobs must be at least 1".to_string());
    }
    Ok(j)
}

#[derive(Debug, Parser)]
#[command(
    name = "vsheet",
    version,
    about = "High-performance Rust CLI to generate clean same-name video contact sheets"
)]
pub struct CliArgs {
    /// Video files or directories to scan (defaults to current directory)
    #[arg(default_value = ".", value_name = "PATH")]
    pub paths: Vec<PathBuf>,

    /// Handling mode when target contact sheet already exists
    #[arg(short = 'm', long = "mode", value_enum, default_value_t = Mode::Skip)]
    pub mode: Mode,

    /// Overwrite existing contact sheets (shortcut for --mode overwrite)
    #[arg(short = 'O', long = "overwrite")]
    pub overwrite: bool,

    /// Grid layout in COLSxROWS format (e.g. 3x3, 4x4, 4x5)
    #[arg(short = 'g', long = "grid", default_value = "4x4")]
    pub grid: GridSpec,

    /// Override grid column count (1..=12)
    #[arg(long = "cols")]
    pub cols: Option<u32>,

    /// Override grid row count (1..=20)
    #[arg(long = "rows")]
    pub rows: Option<u32>,

    /// Output image format
    #[arg(short = 'f', long = "format", value_enum, default_value_t = OutputFormat::Jpg)]
    pub format: OutputFormat,

    /// Custom output file path (single video) or output directory
    #[arg(short = 'o', long = "output", value_name = "OUT_PATH")]
    pub output: Option<PathBuf>,

    /// JPEG encoding quality (1..=100)
    #[arg(short = 'q', long = "quality", default_value_t = 92, value_parser = parse_quality)]
    pub quality: u8,

    /// Output image width in pixels (minimum 800)
    #[arg(short = 'w', long = "width", default_value_t = 1920, value_parser = parse_width)]
    pub width: u32,

    /// Color theme for the contact sheet
    #[arg(short = 't', long = "theme", value_enum, default_value_t = Theme::Apricot)]
    pub theme: Theme,

    /// List all built-in color themes and exit
    #[arg(long = "list-themes")]
    pub list_themes: bool,

    /// Outer canvas padding in pixels
    #[arg(long = "padding", default_value_t = 16)]
    pub padding: u32,

    /// Spacing gap between grid thumbnails in pixels
    #[arg(long = "gap", default_value_t = 10)]
    pub gap: u32,

    /// Worker thread concurrency
    #[arg(short = 'j', long = "jobs", default_value_t = default_jobs(), value_parser = parse_jobs)]
    pub jobs: usize,

    /// Recursively scan subdirectories (enabled by default)
    #[arg(short = 'r', long = "recursive")]
    pub recursive: bool,

    /// Disable recursive traversal (top-level directory only)
    #[arg(short = 'n', long = "no-recursive", conflicts_with = "recursive")]
    pub no_recursive: bool,

    /// Maximum directory traversal depth
    #[arg(long = "max-depth")]
    pub max_depth: Option<usize>,

    /// Include hidden files and directories (names starting with '.')
    #[arg(long = "hidden")]
    pub hidden: bool,

    /// Disable smart black/white/solid frame avoidance (forward-shift)
    #[arg(long = "no-smart-frame")]
    pub no_smart_frame: bool,

    /// Step-forward seconds when a blank frame is detected ('auto' or seconds)
    #[arg(long = "blank-step", default_value_t = BlankStep::Auto)]
    pub blank_step: BlankStep,

    /// Maximum forward-seek attempts per cell when blank frames are encountered
    #[arg(long = "blank-retries", default_value_t = 5)]
    pub blank_retries: u32,

    /// Luminance threshold (0..255) below which a low-contrast frame is treated as black
    #[arg(long = "black-threshold", default_value_t = 18.0, value_parser = parse_threshold)]
    pub black_threshold: f32,

    /// Luminance threshold (0..255) above which a low-contrast frame is treated as white
    #[arg(long = "white-threshold", default_value_t = 238.0, value_parser = parse_threshold)]
    pub white_threshold: f32,

    /// Hide per-cell timestamp badges
    #[arg(long = "no-timestamps")]
    pub no_timestamps: bool,

    /// Custom font file (.ttf/.ttc/.otf) to load with top priority
    #[arg(long = "font", value_name = "FONT_PATH")]
    pub font: Option<PathBuf>,

    /// Scan and print planned tasks without running ffprobe/ffmpeg
    #[arg(long = "dry-run")]
    pub dry_run: bool,
}

impl CliArgs {
    pub fn effective_mode(&self) -> Mode {
        if self.overwrite {
            Mode::Overwrite
        } else {
            self.mode
        }
    }

    pub fn effective_grid(&self) -> Result<GridSpec, String> {
        let cols = self.cols.unwrap_or(self.grid.cols);
        let rows = self.rows.unwrap_or(self.grid.rows);
        GridSpec { cols, rows }.validate()
    }

    pub fn effective_jobs(&self) -> usize {
        self.jobs
    }

    pub fn is_recursive(&self) -> bool {
        !self.no_recursive
    }

    pub fn smart_frame_enabled(&self) -> bool {
        !self.no_smart_frame
    }

    pub fn show_timestamps(&self) -> bool {
        !self.no_timestamps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_allow_zero_arg_invocation() {
        let args = CliArgs::parse_from(["vsheet"]);
        assert_eq!(args.paths, vec![PathBuf::from(".")]);
        assert_eq!(args.effective_mode(), Mode::Skip);
        assert_eq!(args.theme, Theme::Apricot);
        assert!(args.output.is_none());
        assert_eq!(args.padding, 16);
        assert_eq!(args.gap, 10);
        assert_eq!(args.blank_step, BlankStep::Auto);
        assert!(args.is_recursive());
        assert!(args.effective_jobs() >= 1);
    }

    #[test]
    fn recursive_and_no_recursive_flags_work() {
        let rec = CliArgs::parse_from(["vsheet", "-r", "./videos"]);
        assert!(rec.is_recursive());
        let non_rec = CliArgs::parse_from(["vsheet", "-n", "./videos"]);
        assert!(!non_rec.is_recursive());
        let non_rec_long = CliArgs::parse_from(["vsheet", "--no-recursive", "./videos"]);
        assert!(!non_rec_long.is_recursive());
    }

    #[test]
    fn blank_step_and_themes_parse() {
        assert_eq!("auto".parse::<BlankStep>().unwrap(), BlankStep::Auto);
        assert_eq!("AUTO".parse::<BlankStep>().unwrap(), BlankStep::Auto);
        assert_eq!("2.5".parse::<BlankStep>().unwrap(), BlankStep::Fixed(2.5));
        assert!("-1".parse::<BlankStep>().is_err());

        for name in [
            "apricot",
            "gallery",
            "luxury",
            "botanical",
            "ocean",
            "terracotta",
            "rose",
            "frost",
            "galaxy",
            "obsidian",
        ] {
            let args = CliArgs::parse_from(["vsheet", "-t", name, "a.mp4"]);
            assert_eq!(args.theme.name(), name);
        }

        // Backward-compatible aliases
        assert_eq!(
            CliArgs::parse_from(["vsheet", "-t", "light", "a.mp4"]).theme,
            Theme::Gallery
        );
        assert_eq!(
            CliArgs::parse_from(["vsheet", "-t", "dark", "a.mp4"]).theme,
            Theme::Obsidian
        );
        assert_eq!(
            CliArgs::parse_from(["vsheet", "-t", "warm", "a.mp4"]).theme,
            Theme::Apricot
        );

        let with_out = CliArgs::parse_from(["vsheet", "a.mp4", "-o", "out.jpg"]);
        assert_eq!(with_out.output, Some(PathBuf::from("out.jpg")));
    }

    #[test]
    fn grid_spec_parses_valid_values() {
        assert_eq!(
            "3x3".parse::<GridSpec>().unwrap(),
            GridSpec { cols: 3, rows: 3 }
        );
        assert_eq!(
            "4X5".parse::<GridSpec>().unwrap(),
            GridSpec { cols: 4, rows: 5 }
        );
    }

    #[test]
    fn grid_spec_rejects_invalid_values() {
        assert!("0x4".parse::<GridSpec>().is_err());
        assert!("13x4".parse::<GridSpec>().is_err());
        assert!("4x21".parse::<GridSpec>().is_err());
        assert!("4".parse::<GridSpec>().is_err());
    }

    #[test]
    fn overwrite_flag_overrides_mode() {
        let args = CliArgs::parse_from(["vsheet", "--overwrite", "movie.mp4"]);
        assert_eq!(args.effective_mode(), Mode::Overwrite);
        let default_args = CliArgs::parse_from(["vsheet", "movie.mp4"]);
        assert_eq!(default_args.effective_mode(), Mode::Skip);
    }

    #[test]
    fn cols_and_rows_override_grid() {
        let args = CliArgs::parse_from(["vsheet", "-g", "4x4", "--cols", "3", "--rows", "5", "a.mp4"]);
        assert_eq!(
            args.effective_grid().unwrap(),
            GridSpec { cols: 3, rows: 5 }
        );
    }
}
