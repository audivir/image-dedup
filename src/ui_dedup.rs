use crate::{
    ui_utils::{
        App, ConvertItem, LoadTask, LoadedResult, WorkerResult, WorkerTask, init_app, quit_app,
    },
    utils::FileMetadata,
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};
use ratatui_image::{StatefulImage, protocol::StatefulProtocol};
use std::time::Duration;

#[derive(PartialEq, Default)]
enum DedupAppMode {
    #[default]
    Normal,
    ReplaceSelectSource,
    ReplaceSelectTarget(usize),
    Extended {
        index: usize,
        keepers: Vec<bool>,
    },
}

type DedupAppBackend =
    App<Vec<FileMetadata>, Vec<StatefulProtocol>, DedupAppMode, WorkerTask, WorkerResult>;

struct DedupApp {
    app: DedupAppBackend,
}

impl ConvertItem<Vec<FileMetadata>, Vec<StatefulProtocol>, WorkerTask, WorkerResult>
    for DedupAppBackend
{
    fn send_item(&self, index: usize, group: &Vec<FileMetadata>) -> WorkerTask {
        WorkerTask::Load(index, LoadTask::Group(group.clone()))
    }
    fn recv_item(
        &mut self,
        result: WorkerResult,
    ) -> Option<(usize, Vec<FileMetadata>, Vec<Vec<StatefulProtocol>>)> {
        if let WorkerResult::Loaded(index, LoadedResult::Group(group, group_protocols)) = result {
            return Some((index, group, group_protocols));
        }
        None
    }
}

pub(crate) fn run_dedup_ui_app(groups: Vec<Vec<FileMetadata>>) -> Result<()> {
    if groups.is_empty() {
        println!("No similar items found! Exiting.");
        return Ok(());
    }

    let (task_tx, res_rx, mut terminal, current_index) = init_app()?;

    let mut app = DedupApp {
        app: App::new(groups, task_tx, res_rx, current_index),
    };

    loop {
        app.app.store_results();

        terminal.draw(|f| draw_ui(f, &mut app))?;

        // 150ms timeout creates ~6.6 FPS animation matching our ffmpeg output!
        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(key) = event::read()? {
                let current_len = app.app.items[app.app.current].len();

                match &mut app.app.mode {
                    DedupAppMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('s') | KeyCode::Right => {
                            app.app.advance();
                        }
                        KeyCode::Char('d') => {
                            let paths = app.app.items[app.app.current]
                                .iter()
                                .map(|i| i.path.clone())
                                .collect();
                            app.app.task_sender.send(WorkerTask::Delete(paths)).unwrap();
                            app.app.advance();
                        }
                        KeyCode::Char('r') => app.app.mode = DedupAppMode::ReplaceSelectSource,
                        KeyCode::Char('e') => {
                            app.app.mode = DedupAppMode::Extended {
                                index: 0,
                                keepers: vec![false; current_len],
                            }
                        }
                        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                            let selected_idx = c.to_digit(10).unwrap() as usize - 1;
                            if selected_idx < current_len {
                                let to_delete = app.app.items[app.app.current]
                                    .iter()
                                    .enumerate()
                                    .filter(|(i, _)| *i != selected_idx)
                                    .map(|(_, item)| item.path.clone())
                                    .collect();
                                app.app
                                    .task_sender
                                    .send(WorkerTask::Delete(to_delete))
                                    .unwrap();
                                app.app.advance();
                            }
                        }
                        _ => {}
                    },

                    DedupAppMode::ReplaceSelectSource => match key.code {
                        KeyCode::Esc => app.app.mode = DedupAppMode::Normal,
                        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                            let idx = c.to_digit(10).unwrap() as usize - 1;
                            if idx < current_len {
                                app.app.mode = DedupAppMode::ReplaceSelectTarget(idx);
                            }
                        }
                        _ => {}
                    },

                    DedupAppMode::ReplaceSelectTarget(src_idx) => match key.code {
                        KeyCode::Esc => app.app.mode = DedupAppMode::Normal,
                        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                            let target_idx = c.to_digit(10).unwrap() as usize - 1;
                            let src_idx = *src_idx;
                            if target_idx < current_len && target_idx != src_idx {
                                let source_path =
                                    app.app.items[app.app.current][src_idx].path.clone();
                                let target_path =
                                    app.app.items[app.app.current][target_idx].path.clone();

                                let to_delete = app.app.items[app.app.current]
                                    .iter()
                                    .enumerate()
                                    .filter(|(i, _)| *i != src_idx && *i != target_idx)
                                    .map(|(_, item)| item.path.clone())
                                    .collect();

                                app.app
                                    .task_sender
                                    .send(WorkerTask::MoveAndDelete {
                                        source: source_path,
                                        target: target_path,
                                        deletions: to_delete,
                                    })
                                    .unwrap();
                                app.app.advance();
                            }
                        }
                        _ => {}
                    },

                    DedupAppMode::Extended { index, keepers } => match key.code {
                        KeyCode::Esc => app.app.mode = DedupAppMode::Normal,
                        KeyCode::Char('y') => {
                            keepers[*index] = true;
                            if *index + 1 < current_len {
                                *index += 1;
                            }
                        }
                        KeyCode::Char('n') => {
                            keepers[*index] = false;
                            if *index + 1 < current_len {
                                *index += 1;
                            }
                        }
                        KeyCode::Enter => {
                            let to_delete = app.app.items[app.app.current]
                                .iter()
                                .enumerate()
                                .filter(|(i, _)| !keepers[*i])
                                .map(|(_, item)| item.path.clone())
                                .collect();
                            app.app
                                .task_sender
                                .send(WorkerTask::Delete(to_delete))
                                .unwrap();
                            app.app.advance();
                        }
                        _ => {}
                    },
                }
            }
        } else {
            // Tick the animation!
            app.app.animation_tick = app.app.animation_tick.wrapping_add(1);
        }

        if app.app.current >= app.app.items.len() {
            break;
        }
    }

    app.app.task_sender.send(WorkerTask::Quit).unwrap();
    quit_app(&mut terminal)?;
    println!("Done deduping!");
    Ok(())
}

fn draw_ui(f: &mut ratatui::Frame, app: &mut DedupApp) {
    if app.app.current >= app.app.items.len() {
        return;
    }

    let items = &app.app.items[app.app.current];
    let max_key = items.len();

    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(f.area());

    let instructions = match &app.app.mode {
        DedupAppMode::Normal => format!(
            " Group {}/{} | 1-{}: Keep | 's' or '->': Skip | 'd': Delete All | 'r': Replace Path | 'e': Extended Mode | 'q': Quit ",
            app.app.current + 1,
            app.app.items.len(),
            max_key
        ),
        DedupAppMode::ReplaceSelectSource => {
            format!(" REPLACE MODE | Press 1-{} for the file to KEEP", max_key)
        }
        DedupAppMode::ReplaceSelectTarget(src_idx) => format!(
            " REPLACE MODE | Keeping file {}. Press 1-{} for the path to overwrite",
            src_idx + 1,
            max_key
        ),
        DedupAppMode::Extended { index, .. } => format!(
            " EXTENDED MODE | Item {}/{} | 'y': Keep | 'n': Delete | Enter: Confirm | Esc: Cancel ",
            index + 1,
            max_key
        ),
    };

    let help_widget = Paragraph::new(instructions)
        .block(Block::default().borders(Borders::ALL).title("Controls"));
    f.render_widget(help_widget, main_layout[0]);

    let constraints: Vec<Constraint> = items
        .iter()
        .map(|_| Constraint::Ratio(1, items.len() as u32))
        .collect();
    let image_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(main_layout[1]);

    let max_area = items
        .iter()
        .filter(|i| !i.is_video)
        .map(|i| i.width * i.height)
        .max()
        .unwrap_or(0);

    let mut preloaded_protocols = app.app.preloaded.get_mut(&app.app.current).map(|(_, p)| p);

    for (i, (item, area)) in items.iter().zip(image_layout.iter()).enumerate() {
        let key_num = i + 1;
        let size_mb = item.size_bytes as f64 / 1_048_576.0;

        let mut block = Block::default().borders(Borders::ALL).title_bottom(format!(
            " Ratio: {} | Dim: {} | {:.2} MB ",
            item.score, item.dimensions, size_mb
        ));

        if !item.is_video && item.width * item.height == max_area && max_area > 0 {
            block = block.border_style(Style::default().fg(Color::Red));
        }

        match &app.app.mode {
            DedupAppMode::ReplaceSelectTarget(src_idx) if *src_idx == i => {
                block = block
                    .border_style(Style::default().fg(Color::Green))
                    .title_top(" [SOURCE FILE TO KEEP] ");
            }
            DedupAppMode::Extended { index, keepers } => {
                if *index == i {
                    block = block.border_style(Style::default().fg(Color::Yellow));
                }
                let status = if keepers[i] { "[KEEP]" } else { "[DELETE]" };
                block = block.title_top(format!("{} {}", status, item.display_path));
            }
            _ => block = block.title_top(format!("[{}] {}", key_num, item.display_path)),
        }

        let inner_area = block.inner(*area);
        f.render_widget(block, *area);

        let mut rendered = false;
        if let Some(protocols_group) = preloaded_protocols.as_mut() {
            if let Some(protocols) = protocols_group.get_mut(i) {
                if !protocols.is_empty() {
                    // animate by selecting frame based on UI tick
                    let frame_idx = app.app.animation_tick % protocols.len();
                    if let Some(protocol) = protocols.get_mut(frame_idx) {
                        let image_widget = StatefulImage::new();
                        f.render_stateful_widget(image_widget, inner_area, protocol);
                        rendered = true;
                    }
                } else if item.is_video {
                    f.render_widget(Paragraph::new("Loading video thumbnail..."), inner_area);
                    rendered = true;
                }
            }
        }

        if !rendered {
            f.render_widget(Paragraph::new("Loading..."), inner_area);
        }
    }
}
