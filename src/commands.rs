#[cfg(not(feature = "no_colors"))]
use clap::builder::{Styles, styling::AnsiColor};
use czkawka_core::{
    CZKAWKA_VERSION,
    re_exported::{Cropdetect, FilterType, HashAlg},
    tools::similar_videos::{
        ALLOWED_SKIP_FORWARD_AMOUNT, ALLOWED_VID_HASH_DURATION, DEFAULT_SKIP_FORWARD_AMOUNT,
        crop_detect_from_str_opt,
    },
};
use log::error;
use std::path::PathBuf;

#[cfg(not(feature = "no_colors"))]
pub const CLAP_STYLING: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::Cyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default().bold())
    .error(AnsiColor::Red.on_default().bold())
    .valid(AnsiColor::Green.on_default().bold())
    .invalid(AnsiColor::Yellow.on_default().bold());

#[derive(Clone, clap::Parser)]
#[clap(
    name = "czkawka",
    help_template = HELP_TEMPLATE,
    version = CZKAWKA_VERSION,
)]
#[cfg_attr(not(feature = "no_colors"), clap(styles = CLAP_STYLING))]
pub struct Args {
    #[clap(short = 'S', long, help = "Do not dedup but sort images instead")]
    pub sort_mode: bool,
    #[clap(flatten)]
    pub common_cli_items: CommonCliItems,
    #[clap(
        short,
        long,
        value_parser = parse_minimal_file_size,
        default_value = "16384",
        help = "Minimum size in bytes",
        long_help = "Minimum size of checked files in bytes, assigning bigger value may speed up searching"
    )]
    pub minimal_file_size: u64,
    #[clap(
        short = 'i',
        long,
        value_parser = parse_maximal_file_size,
        default_value = "18446744073709551615",
        help = "Maximum size in bytes",
        long_help = "Maximum size of checked files in bytes, assigning lower value may speed up searching"
    )]
    pub maximal_file_size: u64,
    #[clap(
        short = 's',
        long,
        default_value = "5",
        value_parser = clap::value_parser!(u32).range(0..=40),
        help = "Maximum difference between images (0-40)",
        long_help = "Maximum difference between images to be considered as similar (0-40). Lower values mean more strict matching. For hash_size 8, values up to 10 are recommended, for hash_size 16 up to 20 are recommended."
    )]
    pub max_difference: u32,
    #[clap(flatten)]
    pub allow_hard_links: AllowHardLinks,
    #[clap(flatten)]
    pub ignore_same_size: IgnoreSameSize,
    #[clap(flatten)]
    pub ignore_same_resolution: IgnoreSameResolution,
    #[clap(
        short = 'g',
        long,
        default_value = "Gradient",
        value_parser = parse_similar_hash_algorithm,
        help = "Hash algorithm (Mean, Gradient, Blockhash, VertGradient, DoubleGradient, Median)",
        long_help = "Perceptual hash algorithm used to compare images. Gradient (default) works well for most cases, Mean is faster but less accurate, Blockhash is good for finding very similar images, VertGradient/DoubleGradient provide different matching characteristics, Median is robust against color changes."
    )]
    pub hash_alg: HashAlg,
    #[clap(
        short = 'z',
        long,
        default_value = "Nearest",
        value_parser = parse_similar_image_filter,
        help = "Image resize filter (Lanczos3, Nearest, Triangle, Gaussian, CatmullRom)",
        long_help = "Filter algorithm used when resizing images for comparison. Lanczos3 provides highest quality but is slower, Nearest is fastest but lowest quality, Triangle/Gaussian/CatmullRom offer different quality-speed tradeoffs."
    )]
    pub image_filter: FilterType,
    #[clap(
        short = 'c',
        long,
        default_value = "16",
        value_parser = parse_image_hash_size,
        help = "Hash size (8, 16, 32, 64)",
        long_help = "Size of the perceptual hash. Larger values provide more detailed comparison but require higher max_difference values. 8 is fastest and least detailed, 64 is slowest but most detailed. Recommended: 8 or 16 for typical use."
    )]
    pub hash_size: u8,
    #[clap(
        short = 't',
        long,
        value_parser = parse_tolerance,
        default_value = "10",
        help = "Video maximum difference (allowed values <0,20>)",
        long_help = "Maximum difference between video frames, bigger value means that videos can looks more and more different (allowed values <0,20>)"
    )]
    pub tolerance: i32,
    #[clap(
        short = 'U',
        long,
        default_value_t = DEFAULT_SKIP_FORWARD_AMOUNT,
        value_parser = parse_skip_forward_amount,
        help = "Skip forward amount in seconds (allowed values: 0-300, default: 15)",
        long_help = "Amount of seconds to skip forward in video. Allowed values are from 0 to 300. 0 means that no skipping will be done. Default is 15."
    )]
    pub skip_forward_amount: u32,
    #[clap(
        short = 'B',
        long,
        default_value = "letterbox",
        value_parser = parse_crop_detect,
        help = "Crop detect method (none, letterbox, motion)",
        long_help = "Method to detect and crop black bars from video frames before comparison. 'none' disables cropping, 'letterbox' removes static black bars, 'motion' uses motion detection to find content area."
    )]
    pub crop_detect: Cropdetect,
    #[clap(
        short = 'A',
        long,
        default_value = "10",
        value_parser = parse_scan_duration,
        help = "Scan duration in seconds",
        long_help = "Duration of video scanning in seconds. Longer duration provides more accurate results but takes more time. Allowed values are predefined in the application."
    )]
    pub scan_duration: u32,
}

#[derive(Debug, Clone, clap::Args)]
pub struct CommonCliItems {
    #[clap(
        short = 'T',
        long,
        default_value = "0",
        help = "Number of threads to use (0 = all available)",
        long_help = "Limits the number of threads used for scanning. Value 0 (default) will use all available CPU threads. Lower values can reduce CPU usage."
    )]
    pub thread_number: usize,
    #[clap(
        short,
        long,
        required = true,
        help = "Directory(ies) to search",
        long_help = "List of directory(ies) to search (absolute paths). These directories will be scanned but not set as reference folders."
    )]
    pub directories: Vec<PathBuf>,
    #[clap(
        short,
        long,
        help = "Excluded directory(ies)",
        long_help = "List of directory(ies) to exclude from search (absolute paths). Files in these directories will be completely ignored."
    )]
    pub excluded_directories: Vec<PathBuf>,
    #[clap(
        short = 'E',
        long,
        help = "Excluded item(s)",
        long_help = "List of excluded items using wildcards (e.g., */temp*, *.tmp). May be slower than -e, so use -e for directories when possible."
    )]
    pub excluded_items: Vec<String>,
    #[clap(
        short = 'x',
        long,
        help = "Allowed file extension(s)",
        long_help = "List of file extensions to check. Helpful macros are available: IMAGE (jpg,kra,gif,png,bmp,tiff,hdr,svg), TEXT (txt,doc,docx,odt,rtf), VIDEO (mp4,flv,mkv,webm,vob,ogv,gifv,avi,mov,wmv,mpg,m4v,m4p,mpeg,3gp,m2ts), MUSIC (mp3,flac,ogg,tta,wma,webm)"
    )]
    pub allowed_extensions: Vec<String>,
    #[clap(
        short = 'P',
        long,
        help = "Excluded file extension(s)",
        long_help = "List of file extensions to exclude from search."
    )]
    pub excluded_extensions: Vec<String>,
    #[clap(
        short = 'R',
        long,
        help = "Prevents recursive check of folders",
        long_help = "Disables recursive directory traversal. Only files in the top-level directories will be scanned."
    )]
    pub not_recursive: bool,
    #[cfg(target_family = "unix")]
    #[clap(
        short = 'X',
        long,
        help = "Exclude files on other filesystems",
        long_help = "Prevents scanning files on different filesystems (useful to avoid scanning mounted drives, network shares, etc.)"
    )]
    pub exclude_other_filesystems: bool,
    #[clap(flatten)]
    pub do_not_print: DoNotPrint,
    #[clap(
        short = 'W',
        long,
        help = "Ignore error code when files are found",
        long_help = "Suppresses error exit code when duplicate/similar files are found. Useful for scripts that should continue regardless of findings."
    )]
    pub ignore_error_code_on_found: bool,
    #[clap(
        short = 'H',
        long,
        help = "Disable cache",
        long_help = "Disables the cache system. This will make scanning slower but ensures fresh results without cached data."
    )]
    pub disable_cache: bool,
}

#[derive(Debug, clap::Args, Clone, Copy)]
pub struct DoNotPrint {
    #[clap(
        short = 'N',
        long,
        help = "Do not print results to console",
        long_help = "Suppresses printing of search results to the console. Useful when only saving results to files."
    )]
    pub do_not_print_results: bool,
    #[clap(
        short = 'M',
        long,
        help = "Do not print messages to console",
        long_help = "Suppresses all informational messages, warnings, and errors from being printed to console."
    )]
    pub do_not_print_messages: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct AllowHardLinks {
    #[clap(
        short = 'L',
        long,
        help = "Do not ignore hard links",
        long_help = "Treats hard links as separate files rather than ignoring them. By default, hard links are detected and only counted once."
    )]
    pub allow_hard_links: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct CaseSensitiveNameComparison {
    #[clap(
        short = 'l',
        long,
        help = "Use case-sensitive name comparison",
        long_help = "Enables case-sensitive file name comparison. By default, comparisons are case-insensitive (e.g., 'File.txt' equals 'file.txt')."
    )]
    pub case_sensitive_name_comparison: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct IgnoreSameSize {
    #[clap(
        short = 'J',
        long,
        help = "Ignore files with same size",
        long_help = "Groups files by size and keeps only one file from each size group, ignoring files with identical sizes (useful for quick deduplication based solely on file size)."
    )]
    pub ignore_same_size: bool,
}

#[derive(Debug, Clone, clap::Args)]
pub struct IgnoreSameResolution {
    #[clap(
        short = 'Z',
        long,
        help = "Ignore images with same resolution",
        long_help = "Skips images that have identical resolution (width x height), keeping only one image per resolution group."
    )]
    pub ignore_same_resolution: bool,
}

fn parse_scan_duration(s: &str) -> Result<u32, String> {
    match s.parse::<u32>() {
        Ok(scan_duration) => {
            if ALLOWED_VID_HASH_DURATION.contains(&scan_duration) {
                Ok(scan_duration)
            } else {
                Err(format!(
                    "Scan duration must be one of: {ALLOWED_VID_HASH_DURATION:?}"
                ))
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

fn parse_crop_detect(src: &str) -> Result<Cropdetect, String> {
    match crop_detect_from_str_opt(src) {
        Some(crop_detect) => Ok(crop_detect),
        None => Err(format!("Crop detect \"{src}\" is not valid")),
    }
}

fn parse_skip_forward_amount(src: &str) -> Result<u32, String> {
    match src.parse::<u32>() {
        Ok(skip_forward_amount) => {
            if !ALLOWED_SKIP_FORWARD_AMOUNT.contains(&skip_forward_amount) {
                Err(format!(
                    "Skip forward amount must be one of: {ALLOWED_SKIP_FORWARD_AMOUNT:?}"
                ))
            } else {
                Ok(skip_forward_amount)
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

fn parse_tolerance(src: &str) -> Result<i32, &'static str> {
    match src.parse::<i32>() {
        Ok(t) => {
            if (0..=20).contains(&t) {
                Ok(t)
            } else {
                Err("Tolerance should be in range <0,20>(Higher and lower similarity )")
            }
        }
        _ => Err("Failed to parse tolerance as i32 value."),
    }
}

fn parse_minimal_file_size(src: &str) -> Result<u64, String> {
    match src.parse::<u64>() {
        Ok(minimal_file_size) => {
            if minimal_file_size > 0 {
                Ok(minimal_file_size)
            } else {
                Err("Minimum file size must be at least 1 byte".to_string())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

fn parse_maximal_file_size(src: &str) -> Result<u64, String> {
    match src.parse::<u64>() {
        Ok(maximal_file_size) => {
            if maximal_file_size == 0 {
                Err("Maximum file size must be at least 1 byte".to_string())
            } else {
                Ok(maximal_file_size)
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn validate_file_sizes(minimal: u64, maximal: u64) {
    if maximal < minimal {
        error!(
            "WARNING: Maximum file size ({maximal}) is smaller than minimum file size ({minimal}), no files will match."
        );
    }
}

fn parse_similar_image_filter(src: &str) -> Result<FilterType, String> {
    let filter_type = match src.to_lowercase().as_str() {
        "lanczos3" => FilterType::Lanczos3,
        "nearest" => FilterType::Nearest,
        "triangle" => FilterType::Triangle,
        "gaussian" => FilterType::Gaussian,
        "catmullrom" => FilterType::CatmullRom,
        _ => return Err("Couldn't parse the image resize filter (allowed: Lanczos3, Nearest, Triangle, Gaussian, Catmullrom)".to_string()),
    };
    Ok(filter_type)
}

fn parse_similar_hash_algorithm(src: &str) -> Result<HashAlg, String> {
    let algorithm = match src.to_lowercase().as_str() {
        "mean" => HashAlg::Mean,
        "gradient" => HashAlg::Gradient,
        "blockhash" => HashAlg::Blockhash,
        "vertgradient" => HashAlg::VertGradient,
        "doublegradient" => HashAlg::DoubleGradient,
        "median" => HashAlg::Median,
        _ => return Err("Couldn't parse the hash algorithm (allowed: Mean, Gradient, Blockhash, VertGradient, DoubleGradient, Median)".to_string()),
    };
    Ok(algorithm)
}

fn parse_image_hash_size(src: &str) -> Result<u8, String> {
    let hash_size = match src.to_lowercase().as_str() {
        "8" => 8,
        "16" => 16,
        "32" => 32,
        "64" => 64,
        _ => return Err("Couldn't parse the image hash size (allowed: 8, 16, 32, 64)".to_string()),
    };
    Ok(hash_size)
}

const HELP_TEMPLATE: &str = r#"
{bin} {version}

USAGE:
    {usage} [FLAGS] [OPTIONS]

OPTIONS:
{options}

COMMANDS:
{subcommands}

    try "{usage} -h" to get more info about a specific tool

EXAMPLES:
    {bin} image -d /home/rafal -e /home/rafal/Pulpit -f results.txt"#;
