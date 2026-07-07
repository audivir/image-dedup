use crate::{
    commands::{Args, validate_file_sizes},
    sort_media::{SortMedia, SortMediaParameters},
    ui_dedup::run_dedup_ui_app,
    ui_sort::run_sort_ui_app,
    utils::{FileMetadata, run_any_thread, set_common_settings},
};
use crossbeam_channel::Sender;
use czkawka_core::{
    common::{
        config_cache_path::{print_infos_and_warnings, set_config_cache_path},
        consts::VIDEO_FILES_EXTENSIONS,
        logger::{filtering_messages, print_version_mode, setup_logger},
        progress_data::ProgressData,
        tool_data::CommonData,
        traits::{ResultEntry, Search},
    },
    tools::{
        similar_images::{SimilarImages, SimilarImagesParameters},
        similar_videos::{
            DEFAULT_AUDIO_LENGTH_RATIO, DEFAULT_AUDIO_MAXIMUM_DIFFERENCE,
            DEFAULT_AUDIO_MIN_DURATION_SECONDS, DEFAULT_AUDIO_SIMILARITY_PERCENT,
            DEFAULT_THUMBNAIL_GRID_TILES_PER_SIDE, DEFAULT_VIDEO_PERCENTAGE_FOR_THUMBNAIL,
            SimilarVideos, SimilarVideosParameters,
        },
    },
};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

fn run_result_thread<V, F, T>(
    args: Args,
    stop_flag: Arc<AtomicBool>,
    is_video_func: V,
    roots: Vec<(PathBuf, String)>,
    func: F,
) -> Vec<Vec<FileMetadata>>
where
    T: ResultEntry + Send + 'static,
    V: Fn(&Path) -> bool + Send + 'static,
    F: FnOnce(Args, Arc<AtomicBool>, Sender<ProgressData>) -> Vec<Vec<T>> + Send + 'static,
{
    run_any_thread(args, stop_flag, move |args, stop_flag, progress_sender| {
        func(args, stop_flag, progress_sender)
            .iter()
            .map(|g| {
                g.iter()
                    .map(|e| {
                        let path = e.get_path();
                        let is_video = is_video_func(path);
                        FileMetadata::new(path.to_path_buf(), is_video, &roots)
                    })
                    .collect()
            })
            .collect()
    })
}

fn sort_media(
    args: Args,
    stop_flag: &Arc<AtomicBool>,
    progress_sender: &Sender<ProgressData>,
) -> SortMedia {
    let Args {
        sort_mode: _,
        common_cli_items,
        minimal_file_size,
        maximal_file_size,
        max_difference: _,
        allow_hard_links,
        ignore_same_size: _,
        ignore_same_resolution: _,
        hash_alg: _,
        image_filter: _,
        hash_size: _,
        tolerance: _,
        skip_forward_amount: _,
        crop_detect: _,
        scan_duration: _,
    } = args;

    validate_file_sizes(minimal_file_size, maximal_file_size);

    let params = SortMediaParameters::new();
    let mut tool = SortMedia::new(params);

    set_common_settings(&mut tool, &common_cli_items, None);
    tool.set_minimal_file_size(minimal_file_size);
    tool.set_maximal_file_size(maximal_file_size);
    tool.set_hide_hard_links(!allow_hard_links.allow_hard_links);

    tool.search(stop_flag, Some(progress_sender));

    tool
}

fn similar_images(
    args: Args,
    stop_flag: &Arc<AtomicBool>,
    progress_sender: &Sender<ProgressData>,
) -> SimilarImages {
    let Args {
        sort_mode: _,
        common_cli_items,
        minimal_file_size,
        maximal_file_size,
        max_difference,
        allow_hard_links,
        ignore_same_size,
        ignore_same_resolution,
        hash_alg,
        image_filter,
        hash_size,
        tolerance: _,
        skip_forward_amount: _,
        crop_detect: _,
        scan_duration: _,
    } = args;

    validate_file_sizes(minimal_file_size, maximal_file_size);

    let params = SimilarImagesParameters::new(
        max_difference,
        hash_size,
        hash_alg,
        image_filter,
        ignore_same_size.ignore_same_size,
        ignore_same_resolution.ignore_same_resolution,
    );
    let mut tool = SimilarImages::new(params);

    set_common_settings(&mut tool, &common_cli_items, None);
    tool.set_minimal_file_size(minimal_file_size);
    tool.set_maximal_file_size(maximal_file_size);
    tool.set_hide_hard_links(!allow_hard_links.allow_hard_links);

    tool.search(stop_flag, Some(progress_sender));

    tool
}

fn similar_videos(
    args: Args,
    stop_flag: &Arc<AtomicBool>,
    progress_sender: &Sender<ProgressData>,
) -> SimilarVideos {
    let Args {
        sort_mode: _,
        common_cli_items,
        minimal_file_size,
        maximal_file_size,
        max_difference: _,
        allow_hard_links,
        ignore_same_size,
        ignore_same_resolution,
        hash_alg: _,
        image_filter: _,
        hash_size: _,
        tolerance,
        skip_forward_amount,
        crop_detect,
        scan_duration,
    } = args;

    validate_file_sizes(minimal_file_size, maximal_file_size);

    let params = SimilarVideosParameters::new(
        tolerance,
        ignore_same_size.ignore_same_size,
        ignore_same_resolution.ignore_same_resolution,
        skip_forward_amount,
        scan_duration,
        crop_detect,
        false, // generate_thumbnails
        DEFAULT_VIDEO_PERCENTAGE_FOR_THUMBNAIL,
        false, // generate_thumbnail_grid
        DEFAULT_THUMBNAIL_GRID_TILES_PER_SIDE,
        false, // check_audio_content
        DEFAULT_AUDIO_SIMILARITY_PERCENT,
        DEFAULT_AUDIO_MAXIMUM_DIFFERENCE,
        DEFAULT_AUDIO_LENGTH_RATIO,
        DEFAULT_AUDIO_MIN_DURATION_SECONDS,
    );

    let mut tool = SimilarVideos::new(params);
    set_common_settings(&mut tool, &common_cli_items, None);
    tool.set_minimal_file_size(minimal_file_size);
    tool.set_maximal_file_size(maximal_file_size);
    tool.set_hide_hard_links(!allow_hard_links.allow_hard_links);
    tool.search(stop_flag, Some(progress_sender));
    tool
}

fn is_video_ext(path: &Path) -> bool {
    if let Some(extension_str) = path.extension() {
        let ext_lower = extension_str.to_string_lossy().to_lowercase();
        VIDEO_FILES_EXTENSIONS.iter().any(|ext| &ext_lower == ext)
    } else {
        false
    }
}

pub(crate) fn run_sort_app(args: Args, roots: Vec<(PathBuf, String)>, stop_flag: Arc<AtomicBool>) {
    let vec_results = run_result_thread(
        args.clone(),
        stop_flag.clone(),
        is_video_ext,
        roots.clone(),
        |args, stop_flag, progress_sender| {
            let mut tool = sort_media(args, &stop_flag, &progress_sender);
            tool.get_media_files().to_vec()
        },
    );

    let results = vec_results.into_iter().flatten().collect();
    run_sort_ui_app(results).expect("Failed to run app");
}

pub(crate) fn run_dedup_app(args: Args, roots: Vec<(PathBuf, String)>, stop_flag: Arc<AtomicBool>) {
    let config_cache_path_set_result = set_config_cache_path("Czkawka", "Czkawka");
    setup_logger(true, "czkawka_cli", filtering_messages);
    print_version_mode("Czkawka cli");
    print_infos_and_warnings(
        config_cache_path_set_result.infos,
        config_cache_path_set_result.warnings,
    );

    let mut results = run_result_thread(
        args.clone(),
        stop_flag.clone(),
        |_| false,
        roots.clone(),
        |args, stop_flag, progress_sender| {
            let tool = similar_images(args, &stop_flag, &progress_sender);
            tool.get_similar_images().to_vec()
        },
    );

    results.extend(run_result_thread(
        args,
        stop_flag.clone(),
        |_| true,
        roots,
        |args, stop_flag, progress_sender| {
            let tool = similar_videos(args, &stop_flag, &progress_sender);
            tool.get_similar_videos().to_vec()
        },
    ));

    run_dedup_ui_app(results).expect("Failed to run app");
}
