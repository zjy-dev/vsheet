# `vsheet` — 高性能视频联络表 (Contact Sheet) 生成工具

基于 **Rust + Rayon + FFmpeg + resvg** 构建的高性能纯 CLI 视频缩略图预览板生成器。支持单文件或目录输入（递归 / 非递归），默认在视频同目录生成同名切片联络表（如 `movie.mp4` -> `movie.jpg`）。

## 核心特性

- **单文件与目录通吃**：可直接传入单个视频文件、多个文件或整个视频目录（默认当前目录 `.`）；支持 `-r / --recursive`（默认开启，递归子目录）与 `-n / --no-recursive`（仅扫描顶层目录），并支持 `-o / --output` 指定自定义输出路径或目录。
- **设计师级排版与清晰边界**：采用微软雅黑 / 苹方等干净网站无衬线字体，内置主色调强调竖条、三层色彩层次（画布底色 → 顶栏容器 → 独立元信息方格）、柔和高程阴影与双环缩略图边框，每一项元数据与每一帧截图都边界分明。
- **10 套专业设计师配色方案**：涵盖日系暖调、极简画廊、深海军黑金、森系鼠尾草、深海青岚、落日陶土、沙漠玫瑰、极地冰蓝、午夜星云、曜石暗金（`vsheet --list-themes` 查看全部）。
- **双模式增量管理**：
  - `skip` 模式（默认）：自动跳过已存在非空同名联络表的视频，秒级增量扫描。
  - `overwrite` 模式（`-m overwrite` 或 `-O` / `--overwrite`）：强制重新生成并覆盖既有图片。
- **全黑 / 全白 / 纯色帧智能后移**：对每个网格窗口内置中央 70% 区域亮度与方差分析（抗 2.39:1 黑边误判），遇到全黑、过曝全白或纯色转场帧时自动向后步进重试，选取清晰帧并同步更新右下角真实时间戳。
- **多核零落盘高速架构**：`ffprobe` 提取精确元数据（含旋转角度、SAR/DAR、HDR/10-bit、音轨规格），`ffmpeg` 快速关键帧定位直出内存 `rgb24` 管道流，由 `rayon` 线程池并发调度，并通过临时文件原子重命名落盘。

## 依赖要求

系统需安装并可在 `PATH` 中调用：
- `ffmpeg` 与 `ffprobe`

## 编译与安装

```bash
# 编译 Release 版本
cargo build --release

# 安装到 Cargo bin 目录（可直接全局运行 vsheet）
cargo install --path .
```

## 常用示例

```bash
# 1. 直接处理当前目录（默认：递归、skip 跳过已有、4x4、jpg、apricot 暖杏晨光配色）
vsheet

# 2. 处理单个视频文件并指定输出文件
vsheet /path/to/movie.mp4 -o /path/to/preview.jpg

# 3. 仅扫描当前目录顶层视频（非递归），使用 3x3 九宫格 + luxury 黑金奢雅配色
vsheet /path/to/videos -n -g 3x3 -t luxury

# 4. 覆盖已有联络表，改用 gallery 极简画廊白并输出无损 PNG
vsheet /path/to/videos -O -t gallery -f png

# 5. 查看全部内置配色方案
vsheet --list-themes

# 6. 预览将生成与将跳过的列表（不执行 FFmpeg）
vsheet /path/to/videos --dry-run
```

## 参数与默认值一览

| 参数 | 默认值 | 说明 |
| --- | --- | --- |
| `[PATH]...` | `.` | 视频文件或目录路径 |
| `-o, --output <PATH>` | 同目录同名 | 自定义输出文件路径或输出目录 |
| `-m, --mode <skip\|overwrite>` | `skip` | 已存在同名图时的处理方式 |
| `-O, --overwrite` | `false` | 覆盖模式快捷参数 |
| `-r, --recursive` | 默认开启 | 递归扫描子目录 |
| `-n, --no-recursive` | `false` | 禁用递归，仅扫描顶层目录 |
| `-g, --grid <COLSxROWS>` | `4x4` | 网格行列数（如 `3x3`、`4x5`） |
| `-f, --format <jpg\|png>` | `jpg` | 输出图片格式 |
| `-q, --quality <1..100>` | `92` | JPEG 编码质量 |
| `-w, --width <PX>` | `1920` | 输出图片总宽度（最小 `800`） |
| `-t, --theme <THEME>` | `apricot` | 10 套配色之一（`apricot`/`gallery`/`luxury`/`botanical`/`ocean`/`terracotta`/`rose`/`frost`/`galaxy`/`obsidian`） |
| `--padding <PX>` | `16` | 外边距像素 |
| `--gap <PX>` | `10` | 网格缩略图间距像素 |
| `-j, --jobs <N>` | CPU 核心数 | 并发工作线程数 |
| `--no-smart-frame` | `false` | 关闭全黑/全白帧自动后移检测 |
| `--blank-step <auto\|SEC>` | `auto` | 检测到空白帧时的后移步长 |
| `--blank-retries <N>` | `5` | 单个网格最大后移尝试次数 |
| `--black-threshold <0..255>` | `18` | 黑帧亮度判定阈值 |
| `--white-threshold <0..255>` | `238` | 白帧亮度判定阈值 |
| `--no-timestamps` | `false` | 隐藏右下角时间戳角标 |
| `--dry-run` | `false` | 仅打印扫描结果，不实际生成 |
