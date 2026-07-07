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
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};
use ratatui_image::{StatefulImage, protocol::StatefulProtocol};
use std::{fs, path::Path, time::Duration};

#[derive(Default, PartialEq, Clone)]
enum SortAppMode {
    #[default]
    Normal,
    NamingCategory(usize),
    IgnoreDirectory {
        parents: Vec<String>,
        selected: usize,
    },
}

type SortAppBackend = App<FileMetadata, StatefulProtocol, SortAppMode, WorkerTask, WorkerResult>;

struct SortApp {
    app: SortAppBackend,
    categories: [String; 9],
    input_buffer: String,
}

impl ConvertItem<FileMetadata, StatefulProtocol, WorkerTask, WorkerResult> for SortAppBackend {
    fn send_item(&self, index: usize, item: &FileMetadata) -> WorkerTask {
        WorkerTask::Load(index, LoadTask::Item(item.clone()))
    }
    fn recv_item(
        &mut self,
        result: WorkerResult,
    ) -> Option<(usize, FileMetadata, Vec<StatefulProtocol>)> {
        if let WorkerResult::Loaded(index, LoadedResult::Item(item, protocols)) = result {
            return Some((index, item, protocols));
        }
        None
    }
}

pub(crate) fn run_sort_ui_app(items: Vec<FileMetadata>) -> Result<()> {
    if items.is_empty() {
        println!("No images or videos found to sort!");
        return Ok(());
    }

    let (task_tx, res_rx, mut terminal, current_index) = init_app()?;

    let mut default_categories: [String; 9] = Default::default();
    for i in 0..9 {
        default_categories[i] = format!("Folder_{}", i + 1);
    }

    let mut app = SortApp {
        app: App::new(items, task_tx, res_rx, current_index),
        categories: default_categories,
        input_buffer: String::new(),
    };

    loop {
        app.app.store_results();

        terminal.draw(|f| draw_sort_ui(f, &mut app))?;

        // 150ms timeout for GIF/Video animation
        if event::poll(Duration::from_millis(150))? {
            if let Event::Key(key) = event::read()? {
                let current_mode = app.app.mode.clone();
                match current_mode {
                    SortAppMode::Normal => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('s') | KeyCode::Right => app.app.advance(),
                        KeyCode::Left => app.app.go_back(),
                        KeyCode::Char('d') => {
                            let path = app.app.items[app.app.current].path.clone();
                            app.app
                                .task_sender
                                .send(WorkerTask::Delete(vec![path]))
                                .unwrap();
                            app.app.advance();
                        }
                        KeyCode::Char('x') => {
                            let mut parents = Vec::new();
                            let current_display = &app.app.items[app.app.current].display_path;
                            let mut current = std::path::Path::new(current_display).parent();

                            while let Some(p) = current {
                                let p_str = p.to_string_lossy().to_string();
                                if p_str.is_empty() || p_str == "/" || p_str == "\\" {
                                    break;
                                }
                                parents.push(p_str);
                                current = p.parent();
                            }

                            if parents.len() == 1 {
                                let target_dir = &parents[0];
                                let mut next_idx = app.app.current + 1;

                                while next_idx < app.app.items.len() {
                                    let next_display = &app.app.items[next_idx].display_path;
                                    if next_display.starts_with(&format!("{}/", target_dir))
                                        || next_display.starts_with(&format!("{target_dir}\\"))
                                    {
                                        next_idx += 1;
                                    } else {
                                        break;
                                    }
                                }

                                app.app.current = next_idx.saturating_sub(1);
                                app.app.advance();
                            } else if parents.len() > 1 {
                                app.app.mode = SortAppMode::IgnoreDirectory {
                                    parents,
                                    selected: 0,
                                };
                            }
                        }
                        KeyCode::Char('c') => {
                            app.app.mode = SortAppMode::NamingCategory(0); // Start editing category 1
                            app.input_buffer = app.categories[0].clone();
                        }
                        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                            let cat_idx = c.to_digit(10).unwrap() as usize - 1;
                            sort_current_file(&mut app, cat_idx);
                        }
                        _ => {}
                    },
                    SortAppMode::NamingCategory(idx) => match key.code {
                        KeyCode::Esc => app.app.mode = SortAppMode::Normal,
                        KeyCode::Enter => {
                            app.categories[idx] = app.input_buffer.clone();
                            app.app.mode = SortAppMode::Normal;
                        }
                        KeyCode::Backspace => {
                            app.input_buffer.pop();
                        }
                        KeyCode::Char(c) => {
                            app.input_buffer.push(c);
                        }
                        KeyCode::Up => {
                            app.categories[idx] = app.input_buffer.clone();
                            let new_idx = if idx == 0 { 8 } else { idx - 1 };
                            app.app.mode = SortAppMode::NamingCategory(new_idx);
                            app.input_buffer = app.categories[new_idx].clone();
                        }
                        KeyCode::Down => {
                            app.categories[idx] = app.input_buffer.clone();
                            let new_idx = (idx + 1) % 9;
                            app.app.mode = SortAppMode::NamingCategory(new_idx);
                            app.input_buffer = app.categories[new_idx].clone();
                        }
                        _ => {}
                    },
                    SortAppMode::IgnoreDirectory { parents, selected } => match key.code {
                        KeyCode::Esc | KeyCode::Char('q') => app.app.mode = SortAppMode::Normal,
                        KeyCode::Up => {
                            let new_selected = selected.saturating_sub(1);
                            app.app.mode = SortAppMode::IgnoreDirectory {
                                parents,
                                selected: new_selected,
                            };
                        }
                        KeyCode::Down => {
                            let new_selected = (selected + 1).min(parents.len().saturating_sub(1));
                            app.app.mode = SortAppMode::IgnoreDirectory {
                                parents,
                                selected: new_selected,
                            };
                        }
                        KeyCode::Enter => {
                            let target_dir = &parents[selected];
                            let mut next_idx = app.app.current + 1;

                            while next_idx < app.app.items.len() {
                                let next_display = &app.app.items[next_idx].display_path;
                                if next_display.starts_with(&format!("{}/", target_dir))
                                    || next_display.starts_with(&format!("{target_dir}\\"))
                                {
                                    next_idx += 1;
                                } else {
                                    break;
                                }
                            }

                            app.app.current = next_idx.saturating_sub(1);
                            app.app.advance();
                            app.app.mode = SortAppMode::Normal;
                        }
                        _ => {}
                    },
                }
            }
        } else {
            app.app.animation_tick = app.app.animation_tick.wrapping_add(1);
        }

        if app.app.current >= app.app.items.len() {
            break;
        }
    }

    app.app.task_sender.send(WorkerTask::Quit).unwrap();
    quit_app(&mut terminal)?;
    println!("Done sorting!");
    Ok(())
}

fn draw_sort_ui(f: &mut ratatui::Frame, app: &mut SortApp) {
    if app.app.current >= app.app.items.len() {
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(f.area());

    let current_item = &app.app.items[app.app.current];
    let size_mb = current_item.size_bytes as f64 / 1_048_576.0;

    // ================= LEFT: MEDIA PREVIEW =================
    let preview_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(
            " [{}/{}] {} ",
            app.app.current + 1,
            app.app.items.len(),
            current_item.display_path
        ))
        .title_bottom(format!(
            " Dim: {} | {:.2} MB ",
            current_item.dimensions, size_mb
        ));

    let inner_preview = preview_block.inner(chunks[0]);
    f.render_widget(preview_block, chunks[0]);

    if let Some((_, protocols)) = app.app.preloaded.get_mut(&app.app.current) {
        if !protocols.is_empty() {
            let frame_idx = app.app.animation_tick % protocols.len();
            if let Some(protocol) = protocols.get_mut(frame_idx) {
                let image_widget = StatefulImage::new();
                f.render_stateful_widget(image_widget, inner_preview, protocol);
            }
        } else {
            f.render_widget(Paragraph::new("Failed to load media."), inner_preview);
        }
    } else {
        f.render_widget(Paragraph::new("Loading..."), inner_preview);
    }

    // ================= RIGHT: CONTROLS & CATEGORIES =================
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // File Info & Controls
            Constraint::Min(10),   // Categories
        ])
        .split(chunks[1]);

    // 1. Controls Info
    let controls_text = vec![
        Line::from(vec![Span::styled(
            "Controls:",
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from(" [1-9] Sort into folder"),
        Line::from(" [S] / [->] Skip"),
        Line::from(" [<-] Go Back"),
        Line::from(" [D] Delete File"),
        Line::from(" [X] Ignore Folder"),
        Line::from(" [C] Rename Folders"),
        Line::from(" [Q] Quit"),
    ];
    let controls_block = Paragraph::new(controls_text)
        .block(Block::default().borders(Borders::ALL).title(" Actions "));
    f.render_widget(controls_block, right_chunks[0]);

    // 2. Categories List (Or Ignore Menu)
    let mut cat_lines = Vec::new();

    if let SortAppMode::IgnoreDirectory { parents, selected } = &app.app.mode {
        cat_lines.push(Line::from(vec![Span::styled(
            "Select Directory to Ignore:",
            Style::default().add_modifier(Modifier::BOLD).fg(Color::Red),
        )]));
        cat_lines.push(Line::from(Span::styled(
            " (Up/Down to select, Enter to skip all)",
            Style::default().fg(Color::DarkGray),
        )));
        cat_lines.push(Line::from(""));

        for (i, p) in parents.iter().enumerate() {
            let prefix = if i == *selected { "> " } else { "  " };
            let style = if i == *selected {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            cat_lines.push(Line::from(Span::styled(format!("{}{}", prefix, p), style)));
        }
    } else {
        cat_lines.push(Line::from(vec![Span::styled(
            "Target Folders:",
            Style::default().add_modifier(Modifier::BOLD),
        )]));
        cat_lines.push(Line::from(""));

        for i in 0..9 {
            let is_editing = match app.app.mode {
                SortAppMode::NamingCategory(idx) if idx == i => true,
                _ => false,
            };

            let num_span = Span::styled(format!("{}: ", i + 1), Style::default().fg(Color::Yellow));

            if is_editing {
                cat_lines.push(Line::from(vec![
                    num_span,
                    Span::styled(
                        format!("{}█", app.input_buffer),
                        Style::default().fg(Color::Green),
                    ),
                ]));
                cat_lines.push(Line::from(Span::styled(
                    "   (Up/Down arrows to switch, Enter to save)",
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                cat_lines.push(Line::from(vec![num_span, Span::raw(&app.categories[i])]));
            }
        }
    }

    let categories_block = Paragraph::new(cat_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Sort Destinations "),
    );
    f.render_widget(categories_block, right_chunks[1]);
}

fn sort_current_file(app: &mut SortApp, cat_idx: usize) {
    let source = app.app.items[app.app.current].path.clone();
    let target_dir_name = &app.categories[cat_idx];

    // Sort relative to the current working directory
    let target_dir = Path::new(target_dir_name);
    if !target_dir.exists() {
        let _ = fs::create_dir_all(target_dir);
    }

    if let Some(file_name) = source.file_name() {
        let target_file = target_dir.join(file_name);
        app.app
            .task_sender
            .send(WorkerTask::MoveAndDelete {
                source,
                target: target_file,
                deletions: vec![],
            })
            .unwrap();
    }

    app.app.advance();
}
