use crate::{commands::CommonCliItems, progress::connect_progress};
use crossbeam_channel::{Receiver, Sender, unbounded};
use czkawka_core::common::{
    consts::DEFAULT_THREAD_SIZE, progress_data::ProgressData, tool_data::CommonData, traits::Search,
};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    thread,
};

#[derive(Clone)]
pub struct FileMetadata {
    pub path: PathBuf,
    pub display_path: String,
    pub size_bytes: u64,
    pub width: u32,
    pub height: u32,
    pub dimensions: String,
    pub score: String,
    pub is_video: bool,
}

impl FileMetadata {
    pub fn new(path: PathBuf, is_video: bool, roots: &Vec<(PathBuf, String)>) -> Self {
        let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

        let mut display_path = path.display().to_string();
        let c_path = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());

        for (root, prefix) in roots {
            if let Ok(rel) = c_path.strip_prefix(root) {
                display_path = format!("{}{}", prefix, rel.display());
                break;
            }
        }

        FileMetadata {
            path: path,
            display_path,
            size_bytes: size,
            width: 0,
            height: 0,
            dimensions: "Loading...".to_string(),
            score: if is_video {
                "-".to_string()
            } else {
                "Loading...".to_string()
            },
            is_video,
        }
    }

    pub fn area(&self) -> u32 {
        if !self.is_video && self.width > 0 && self.height > 0 {
            self.width * self.height
        } else {
            0
        }
    }

    pub fn set_score(&mut self, max_area: u32, max_size: u64) {
        if self.is_video {
            return;
        }

        if self.width == 0 {
            self.score = "Corrupt".to_string();
            return;
        }

        let area = self.area();
        let area_ratio = if max_area > 0 {
            area as f64 / max_area as f64
        } else {
            1.0
        };
        let size_ratio = if max_size > 0 {
            self.size_bytes as f64 / max_size as f64
        } else {
            1.0
        };
        self.score = format!("{:.0}%", area_ratio.min(size_ratio) * 100.0);
    }
}

pub(crate) fn run_any_thread<F, T, R>(input: T, stop_flag: Arc<AtomicBool>, func: F) -> R
where
    T: Send + 'static,
    R: Send + 'static,
    F: FnOnce(T, Arc<AtomicBool>, Sender<ProgressData>) -> R + Send + 'static,
{
    let (progress_sender, progress_receiver): (Sender<ProgressData>, Receiver<ProgressData>) =
        unbounded();

    let calculate_thread = thread::Builder::new()
        .stack_size(DEFAULT_THREAD_SIZE)
        .spawn(move || func(input, stop_flag, progress_sender))
        .expect("Failed to spawn calculation thread");

    connect_progress(&progress_receiver);

    let results = calculate_thread
        .join()
        .expect("Failed to join calculation thread");

    results
}

pub(crate) fn canonicalize_dirs(dirs: &[PathBuf]) -> Vec<(PathBuf, String)> {
    let mut canon_dirs = Vec::new();
    for d in dirs {
        if let Ok(c) = fs::canonicalize(d) {
            canon_dirs.push(c);
        } else {
            canon_dirs.push(d.clone());
        }
    }

    let mut roots = Vec::new();
    if canon_dirs.len() == 1 {
        roots.push((canon_dirs[0].clone(), String::new()));
    } else {
        let mut name_counts = HashMap::new();
        for (i, d) in canon_dirs.iter().enumerate() {
            let name = d
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| i.to_string());
            *name_counts.entry(name).or_insert(0) += 1;
        }

        for (i, d) in canon_dirs.iter().enumerate() {
            let name = d
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| i.to_string());
            if name_counts[&name] > 1 {
                roots.push((d.clone(), format!("{{{}}}/", i + 1)));
            } else {
                roots.push((d.clone(), format!("{}/", name)));
            }
        }
    }

    roots.sort_by(|a, b| b.0.as_os_str().len().cmp(&a.0.as_os_str().len()));

    roots
}

/// Czkawka's set_common_settings without setting the thread number and requiring less traits
pub fn set_common_settings<T>(
    component: &mut T,
    common_cli_items: &CommonCliItems,
    reference_directories: Option<&Vec<PathBuf>>,
) where
    T: CommonData + Search,
{
    let mut included_directories = common_cli_items.directories.clone();
    if let Some(reference_directories) = reference_directories {
        included_directories.extend_from_slice(reference_directories);
        component.set_reference_paths(reference_directories.clone());
    }

    component.set_included_paths(included_directories);
    component.set_excluded_paths(common_cli_items.excluded_directories.clone());
    component.set_excluded_items(common_cli_items.excluded_items.clone());
    component.set_recursive_search(!common_cli_items.not_recursive);
    #[cfg(target_family = "unix")]
    component.set_exclude_other_filesystems(common_cli_items.exclude_other_filesystems);
    component.set_allowed_extensions(common_cli_items.allowed_extensions.clone());
    component.set_excluded_extensions(common_cli_items.excluded_extensions.clone());
    component.set_use_cache(!common_cli_items.disable_cache);
}
