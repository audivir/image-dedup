use crossbeam_channel::Sender;
use czkawka_core::common::{
    consts::{
        HEIC_EXTENSIONS, IMAGE_RS_SIMILAR_IMAGES_EXTENSIONS, RAW_IMAGE_EXTENSIONS,
        VIDEO_FILES_EXTENSIONS,
    },
    dir_traversal::{DirTraversalBuilder, DirTraversalResult, inode, take_1_per_inode},
    model::{FileEntry, ToolType, WorkContinueStatus},
    progress_data::{CurrentStage, ProgressData},
    progress_stop_handler::{check_if_stop_received, prepare_thread_handler_common},
    tool_data::{CommonData, CommonToolData},
    traits::Search,
};
use rayon::prelude::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

#[derive(Default, Clone, Copy)]
pub struct Info {
    pub number_of_files: usize,
    pub scanning_time: Duration,
}

#[derive(Clone)]
pub struct SortMediaParameters {}

impl SortMediaParameters {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct SortMedia {
    common_data: CommonToolData,
    information: Info,
    media_files: BTreeMap<String, FileEntry>,
    media_files_vec: Vec<Vec<FileEntry>>,
    params: SortMediaParameters,
}

impl SortMedia {
    pub fn new(params: SortMediaParameters) -> Self {
        Self {
            common_data: CommonToolData::new(ToolType::SimilarImages),
            information: Default::default(),
            media_files: Default::default(),
            media_files_vec: Default::default(),
            params,
        }
    }

    fn update_media_files(&mut self) {
        self.media_files_vec = self
            .media_files
            .clone()
            .into_values()
            .map(|fe| {
                let mut entries = Vec::new();
                entries.push(fe);
                entries
            })
            .collect();
    }
    pub fn get_media_files(&mut self) -> &Vec<Vec<FileEntry>> {
        self.update_media_files();
        &self.media_files_vec
    }
}

impl Search for SortMedia {
    fn search(
        &mut self,
        stop_flag: &std::sync::Arc<std::sync::atomic::AtomicBool>,
        progress_sender: Option<
            &crossbeam_channel::Sender<czkawka_core::common::progress_data::ProgressData>,
        >,
    ) {
        let start_time = Instant::now();

        let () = (|| {
            let extensions = if cfg!(feature = "heif") {
                [
                    IMAGE_RS_SIMILAR_IMAGES_EXTENSIONS,
                    RAW_IMAGE_EXTENSIONS,
                    VIDEO_FILES_EXTENSIONS,
                    HEIC_EXTENSIONS,
                ]
                .concat()
            } else {
                [
                    IMAGE_RS_SIMILAR_IMAGES_EXTENSIONS,
                    RAW_IMAGE_EXTENSIONS,
                    VIDEO_FILES_EXTENSIONS,
                ]
                .concat()
            };

            if self.prepare_items(Some(&extensions)).is_err() {
                return;
            }
            self.common_data.use_reference_folders = !self
                .common_data
                .directories
                .reference_directories
                .is_empty()
                || !self.common_data.directories.reference_files.is_empty();
            if self.check_for_media_files(stop_flag, progress_sender) == WorkContinueStatus::Stop {
                self.common_data.stopped_search = true;
                return;
            }
            // if self.hash_images(stop_flag, progress_sender) == WorkContinueStatus::Stop {
            //     self.common_data.stopped_search = true;
            //     return;
            // }
            // if self.find_similar_hashes(stop_flag, progress_sender) == WorkContinueStatus::Stop {
            //     self.common_data.stopped_search = true;
            //     return;
            // }
            // if self.delete_files(stop_flag, progress_sender) == WorkContinueStatus::Stop {
            //     self.common_data.stopped_search = true;
            // }
        })();

        self.information.scanning_time = start_time.elapsed();

        if !self.common_data.stopped_search {
            println!("Not stopped");
            // self.debug_print();
        }
    }
}

impl CommonData for SortMedia {
    type Info = Info;
    type Parameters = SortMediaParameters;

    fn get_information(&self) -> Self::Info {
        self.information
    }
    fn get_params(&self) -> Self::Parameters {
        self.params.clone()
    }
    fn get_cd(&self) -> &CommonToolData {
        &self.common_data
    }
    fn get_cd_mut(&mut self) -> &mut CommonToolData {
        &mut self.common_data
    }
    fn found_any_items(&self) -> bool {
        self.information.number_of_files > 0
    }
}

impl SortMedia {
    pub(crate) fn check_for_media_files(
        &mut self,
        stop_flag: &Arc<AtomicBool>,
        progress_sender: Option<&Sender<ProgressData>>,
    ) -> WorkContinueStatus {
        let result = DirTraversalBuilder::new()
            .group_by(inode)
            .stop_flag(stop_flag)
            .progress_sender(progress_sender)
            .common_data(&self.common_data)
            .build()
            .run();

        match result {
            DirTraversalResult::SuccessFiles {
                grouped_file_entries,
                warnings,
            } => {
                self.common_data.text_messages.warnings.extend(warnings);

                let progress_handler = prepare_thread_handler_common(
                    progress_sender,
                    CurrentStage::SimilarImagesHidingHardLinks,
                    grouped_file_entries.len(),
                    self.get_test_type(),
                    0,
                );
                let hide_hard_links = self.get_hide_hard_links();
                self.media_files = grouped_file_entries
                    .into_par_iter()
                    .map(|(inode, fes)| {
                        if check_if_stop_received(stop_flag) {
                            return None;
                        }
                        progress_handler.increase_items(1);
                        Some((inode, fes))
                    })
                    .while_some()
                    .flat_map(if hide_hard_links {
                        |(_, fes)| fes
                    } else {
                        take_1_per_inode
                    })
                    .map(|fe| {
                        let fe_str = fe.path.to_string_lossy().to_string();

                        (fe_str, fe)
                    })
                    .collect();

                progress_handler.join_thread();

                if check_if_stop_received(stop_flag) {
                    return WorkContinueStatus::Stop;
                }

                self.information.number_of_files = self.media_files.len();

                // debug!(
                //     "check_files - Found {} image files.",
                //     self.images_to_check.len()
                // );
                WorkContinueStatus::Stop
            }

            DirTraversalResult::Stopped => WorkContinueStatus::Stop,
        }
    }
}
