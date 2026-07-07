use crate::utils::FileMetadata;
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, unbounded};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use image::{AnimationDecoder, DynamicImage, codecs::gif::GifDecoder};
use ratatui::{Terminal, backend::CrosstermBackend};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};
use rayon::prelude::*;
use std::{
    collections::HashMap,
    fs,
    io::{self, Stdout},
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

pub(crate) enum LoadTask {
    Group(Vec<FileMetadata>),
    Item(FileMetadata),
}

pub(crate) enum LoadedResult {
    Group(Vec<FileMetadata>, Vec<Vec<StatefulProtocol>>),
    Item(FileMetadata, Vec<StatefulProtocol>),
}

pub(crate) enum WorkerTask {
    Load(usize, LoadTask),
    Delete(Vec<PathBuf>),
    MoveAndDelete {
        source: PathBuf,
        target: PathBuf,
        deletions: Vec<PathBuf>,
    },
    Quit,
}

pub(crate) enum WorkerResult {
    Loaded(usize, LoadedResult),
}

pub(crate) trait ConvertItem<T, P, S, R> {
    fn send_item(&self, index: usize, item: &T) -> S;
    fn recv_item(&mut self, result: R) -> Option<(usize, T, Vec<P>)>;
}

pub(crate) struct App<T, P, M, S, R> {
    pub items: Vec<T>,
    pub current: usize,
    pub preloaded: HashMap<usize, (T, Vec<P>)>,
    pub mode: M,
    pub task_sender: Sender<S>,
    pub result_recv: Receiver<R>,
    pub animation_tick: usize,
    pub current_index: Arc<AtomicUsize>,
}

impl<T, P, M, S, R> App<T, P, M, S, R>
where
    T: Clone,
    M: Default,
    Self: ConvertItem<T, P, S, R>,
{
    pub fn new(
        items: Vec<T>,
        task_sender: Sender<S>,
        result_recv: Receiver<R>,
        current_index: Arc<AtomicUsize>,
    ) -> Self {
        let app = Self {
            items,
            current: 0,
            preloaded: Default::default(),
            mode: Default::default(),
            task_sender,
            result_recv,
            animation_tick: 0,
            current_index,
        };

        // Aggressively preload initially
        for i in 0..5 {
            app.request_load(i);
        }

        app
    }

    pub fn reset_state(&mut self) {
        self.mode = Default::default();
        self.animation_tick = 0;
    }

    pub fn request_load(&self, index: usize) {
        if index < self.items.len() && !self.preloaded.contains_key(&index) {
            self.task_sender
                .send(self.send_item(index, &self.items[index]))
                .unwrap();
        }
    }

    pub fn store_results(&mut self) {
        while let Ok(result) = self.result_recv.try_recv() {
            if let Some((index, item, protocols)) = self.recv_item(result)
                && index < self.items.len()
            {
                self.items[index] = item.clone();
                self.preloaded.insert(index, (item, protocols));
            }
        }
    }

    pub fn advance(&mut self) {
        self.current += 1;
        self.current_index.store(self.current, Ordering::Relaxed);
        self.reset_state();

        // Cleanup old protocols to save RAM
        let keep_start = self.current.saturating_sub(1);
        self.preloaded.retain(|&k, _| k >= keep_start);

        // Aggressively request next loads
        for i in 0..5 {
            self.request_load(self.current + i);
        }
    }

    #[allow(dead_code)]
    pub fn go_back(&mut self) {
        self.current = self.current.saturating_sub(1);
        self.current_index.store(self.current, Ordering::Relaxed);
        self.reset_state();

        for i in 0..5 {
            self.request_load(self.current + i);
        }
    }
}

fn worker_thread(
    task_rx: Receiver<WorkerTask>,
    res_tx: Sender<WorkerResult>,
    picker: Picker,
    current_index: Arc<AtomicUsize>,
) {
    while let Ok(task) = task_rx.recv() {
        match task {
            WorkerTask::Load(idx, load_task) => {
                // If user has quickly skipped past this task, cancel processing to save resources
                if idx < current_index.load(Ordering::Relaxed) {
                    continue;
                }

                let result = match load_task {
                    LoadTask::Group(mut items) => {
                        let group_protocols = decode_group_to_protocols(&mut items, &picker);
                        LoadedResult::Group(items, group_protocols)
                    }
                    LoadTask::Item(mut item) => {
                        let protocols = decode_item_to_protocols(&mut item, &picker);
                        LoadedResult::Item(item, protocols)
                    }
                };

                // Final check before sending back, purely optional but good for rapid skipping
                if idx >= current_index.load(Ordering::Relaxed) {
                    let _ = res_tx.send(WorkerResult::Loaded(idx, result));
                }
            }
            WorkerTask::Delete(paths) => {
                for p in paths {
                    let _ = fs::remove_file(p);
                }
            }
            WorkerTask::MoveAndDelete {
                source,
                target,
                deletions,
            } => {
                let _ = fs::rename(&source, &target);
                for p in deletions {
                    let _ = fs::remove_file(p);
                }
            }
            WorkerTask::Quit => break,
        }
    }
}

pub(crate) fn init_app() -> Result<(
    Sender<WorkerTask>,
    Receiver<WorkerResult>,
    Terminal<CrosstermBackend<Stdout>>,
    Arc<AtomicUsize>,
)> {
    let (task_tx, task_rx) = unbounded();
    let (res_tx, res_rx) = unbounded();
    let current_index = Arc::new(AtomicUsize::new(0));
    let worker_current_index = current_index.clone();

    let picker = Picker::from_query_stdio()?;

    thread::spawn(move || worker_thread(task_rx, res_tx, picker, worker_current_index));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let terminal = Terminal::new(CrosstermBackend::new(stdout))?;

    Ok((task_tx, res_rx, terminal, current_index))
}

pub(crate) fn quit_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

/// Extract duration with ffprobe
pub(crate) fn get_duration(path: &PathBuf) -> Option<String> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(&path)
        .output();

    if let Ok(out) = output
        && let Ok(duration_str) = String::from_utf8(out.stdout)
        && let Ok(duration_secs) = duration_str.trim().parse::<f64>()
    {
        let mins = (duration_secs / 60.0).floor() as u32;
        let secs = (duration_secs % 60.0).floor() as u32;
        return Some(format!("{}m {:02}s", mins, secs));
    }

    None
}

/// Generate quick GIF
pub(crate) fn get_gif(path: &PathBuf) -> PathBuf {
    let temp_dir = std::env::temp_dir();
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    let temp_gif = temp_dir.join(format!("czk_vid_{}.gif", hasher.finish()));

    if !temp_gif.exists() {
        let _ = Command::new("ffmpeg")
            .arg("-y")
            .arg("-ss")
            .arg("00:00:00") // Start time
            .arg("-t")
            .arg("2") // 2 seconds duration
            .arg("-i")
            .arg(&path)
            .arg("-vf")
            .arg("fps=6,scale=320:-1:flags=lanczos")
            .arg("-loop")
            .arg("0")
            .arg(&temp_gif)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    temp_gif
}

pub(crate) fn push_frame(
    dyn_img: DynamicImage,
    is_empty: bool,
    is_video: bool,
) -> (DynamicImage, Option<(u32, u32, String)>) {
    let mut metadata: Option<(u32, u32, String)> = None;

    if is_empty && !is_video {
        let width = dyn_img.width();
        let height = dyn_img.height();
        let dimensions = format!("{}x{}", width, height);
        metadata = Some((width, height, dimensions));
    }
    if dyn_img.width() > 1000 || dyn_img.height() > 1000 {
        (dyn_img.thumbnail(1000, 1000), metadata)
    } else {
        (dyn_img, metadata)
    }
}
pub(crate) fn render_gif_frames(
    frames: &mut Vec<DynamicImage>,
    path: &PathBuf,
    is_video: bool,
) -> Option<(u32, u32, String)> {
    let mut metadata: Option<(u32, u32, String)> = None;

    if let Ok(file) = fs::File::open(&path) {
        // wrap the file in a BufReader
        let reader = io::BufReader::new(file);

        // pass the buffered reader to the decoder
        if let Ok(decoder) = GifDecoder::new(reader) {
            if let Ok(decoded_frames) = decoder.into_frames().collect_frames() {
                for frame in decoded_frames {
                    let full_img = DynamicImage::ImageRgba8(frame.into_buffer());
                    let (scaled_img, metadata_opt) =
                        push_frame(full_img, frames.is_empty(), is_video);
                    metadata = metadata.or(metadata_opt);
                    frames.push(scaled_img);
                }
            }
        }
    }

    metadata
}

pub(crate) fn process_single_image(item: &mut FileMetadata) -> Vec<DynamicImage> {
    let mut frames = Vec::new();
    let mut file_to_open = item.path.clone();
    let mut is_gif = file_to_open
        .extension()
        .map_or(false, |ext| ext.to_ascii_lowercase() == "gif");

    if item.is_video {
        item.dimensions = get_duration(&item.path).unwrap_or("No duration available".into());
        file_to_open = get_gif(&item.path);
        is_gif = true;
    }

    let mut metadata: Option<(u32, u32, String)> = None;

    // decode image / animation
    if is_gif {
        metadata = render_gif_frames(&mut frames, &file_to_open, item.is_video);
    }

    // fallback to static if GIF extraction failed, or if it's a regular PNG/JPG
    if frames.is_empty() {
        if let Ok(full_img) = image::open(&file_to_open) {
            let (scaled_img, metadata_opt) = push_frame(full_img, true, item.is_video);
            metadata = metadata_opt;
            frames.push(scaled_img);
        }
    }

    if let Some((w, h, d)) = metadata {
        item.width = w;
        item.height = h;
        item.dimensions = d;
    }

    frames
}

/// Multithreaded Processing (ffmpeg + decoding)
pub(crate) fn process_images(items: &mut Vec<FileMetadata>) -> Vec<Vec<DynamicImage>> {
    items
        .par_iter_mut()
        .map(|item| process_single_image(item))
        .collect()
}

fn decode_item_to_protocols(item: &mut FileMetadata, picker: &Picker) -> Vec<StatefulProtocol> {
    let frames = process_single_image(item);

    frames
        .into_iter()
        .map(|img| picker.new_resize_protocol(img))
        .collect()
}

fn decode_group_to_protocols(
    items: &mut Vec<FileMetadata>,
    picker: &Picker,
) -> Vec<Vec<StatefulProtocol>> {
    let processed_frames = process_images(items);

    let mut group_protocols = Vec::new();
    let mut max_area = 0;
    let mut max_size = 0;

    for (item, frames) in items.iter_mut().zip(processed_frames) {
        let mut item_protocols = Vec::new();

        for dyn_img in frames {
            item_protocols.push(picker.new_resize_protocol(dyn_img));
        }

        max_area = max_area.max(item.area());
        max_size = max_size.max(item.size_bytes);

        group_protocols.push(item_protocols);
    }

    for item in items {
        item.set_score(max_area, max_size);
    }

    group_protocols
}
