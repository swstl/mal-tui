use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Gauge, Padding, Paragraph, Wrap},
};
use std::sync::{Arc, Mutex};
// Assuming these imports exist based on your provided code
use crate::{
    app::{Action, ExtraInfo},
    config::{Config, navigation::NavDirection},
    mal::models::anime::Anime,
    screens::widgets::navigatable::Navigatable,
    utils::imageManager::ImageManager,
};
use crossterm::event::{KeyEvent, MouseEvent, MouseEventKind};

#[derive(Clone, Copy, PartialEq, Eq)]
enum FocusedElement {
    SyncButtons,
    BatchButton,
    BackButton,
}

type SyncOrDelete = (bool, Anime);

#[derive(Clone)]
pub struct SyncPopup {
    toggled: bool,
    syncing: bool,
    animes_to_sync: Vec<Anime>,
    to_be_processed: Vec<SyncOrDelete>,
    finished_processing: Vec<SyncOrDelete>,
    current_anime: usize,
    info: ExtraInfo,
    focus: FocusedElement,

    // UI Components
    buttons: Vec<String>,
    nav: Navigatable,
    image_manager: Arc<Mutex<ImageManager>>,
    all_selected: bool,

    // Button areas for mouse handling
    back_btn_area: Option<Rect>,
    batch_btn_area: Option<Rect>,
}

impl SyncPopup {
    pub fn new(info: ExtraInfo) -> Self {
        let buttons = vec![
            "Sync to MAL".to_string(),
            "Remove local changes".to_string(),
        ];

        let image_manager = Arc::new(Mutex::new(ImageManager::new()));

        ImageManager::init_with_threads(&image_manager, info.app_sx.clone());

        Self {
            toggled: false,
            syncing: false,
            current_anime: 0,
            animes_to_sync: Vec::new(),
            to_be_processed: Vec::new(),
            finished_processing: Vec::new(),
            nav: Navigatable::new((1, buttons.len() as u16)), // 2 columns, 1 row
            buttons,
            image_manager,
            info,
            all_selected: false,
            focus: FocusedElement::SyncButtons,
            back_btn_area: None,
            batch_btn_area: None,
        }
    }

    pub fn open(&mut self) -> &Self {
        if !self.animes_to_sync.is_empty() {
            self.toggled = true;
        }
        self
    }

    fn change_button_lables(&mut self) {
        if self.all_selected {
            self.buttons = vec![
                "Sync to MAL (all)".to_string(),
                "Remove local changes (all)".to_string(),
            ];
        } else {
            self.buttons = vec![
                "Sync to MAL".to_string(),
                "Remove local changes".to_string(),
            ];
        }
    }

    pub fn finished_syncing(&mut self, anime: Anime) {
        self.finished_processing.push((true, anime));
    }
    pub fn finished_deleting(&mut self, anime: Anime) {
        self.finished_processing.push((false, anime));
    }

    pub fn next(&mut self) {
        self.current_anime += 1;
    }
    pub fn back(&mut self) {
        self.current_anime = self.current_anime.saturating_sub(1);
        self.to_be_processed.pop();
    }

    pub fn done(&mut self) {
        self.current_anime = self.animes_to_sync.len();
    }

    pub fn clear(&mut self) {
        self.animes_to_sync.clear();
        self.syncing = false;
    }

    pub fn is_open(&self) -> bool {
        self.toggled
    }

    pub fn close(&mut self) -> &Self {
        self.toggled = false;
        self
    }

    pub fn set_animes(&mut self, animes: Vec<Anime>) -> &Self {
        self.animes_to_sync = animes;
        // If we have animes and we are open, ensure navigation is reset
        if !self.animes_to_sync.is_empty() {
            self.nav.back_to_start();
        }
        self
    }

    /// Helper to get the current anime being viewed
    fn current_anime(&self) -> Option<&Anime> {
        self.animes_to_sync.get(self.current_anime)
    }

    fn handle_sync_or_delete(&mut self, anime: Anime) -> Option<Action> {
        if self.all_selected {
            match self.nav.get_selected_index() {
                0 => {
                    for anime in &self.animes_to_sync[self.current_anime..] {
                        self.to_be_processed.push((true, anime.clone()));
                    }
                }
                1 => {
                    for anime in &self.animes_to_sync[self.current_anime..] {
                        self.to_be_processed.push((false, anime.clone()));
                    }
                }
                _ => {}
            }

            self.done();
            return Some(Action::SyncAnimes(self.to_be_processed.clone()));
        } else {
            match self.nav.get_selected_index() {
                0 => {
                    self.to_be_processed.push((true, anime));
                }
                1 => {
                    self.to_be_processed.push((false, anime));
                }
                _ => {}
            }

            self.next();

            if self.to_be_processed.len() == self.animes_to_sync.len() {
                return Some(Action::SyncAnimes(self.to_be_processed.clone()));
            }
        }

        None
    }

    pub fn handle_keyboard(&mut self, key_event: KeyEvent) -> Option<Action> {
        if !self.toggled {
            return None;
        }

        // If showing summary, any key closes (only when sync is complete)
        if self.current_anime().is_none() {
            if self.finished_processing.len() >= self.to_be_processed.len() {
                self.close();
                self.current_anime = 0;
                self.to_be_processed.clear();
                self.finished_processing.clear();
                self.animes_to_sync.clear();
            }
            return None;
        }

        let nav_config = &Config::global().navigation;

        match self.focus {
            FocusedElement::BackButton => {
                match nav_config.get_direction(&key_event.code) {
                    NavDirection::Up => self.focus = FocusedElement::BatchButton,
                    NavDirection::Down | NavDirection::Left | NavDirection::Right => {
                        self.focus = FocusedElement::SyncButtons
                    }
                    _ => {}
                }

                if nav_config.is_select(&key_event.code) {
                    self.back();
                    if self.current_anime == 0 {
                        self.focus = FocusedElement::SyncButtons;
                    }
                }
            }

            FocusedElement::BatchButton => {
                match nav_config.get_direction(&key_event.code) {
                    NavDirection::Down if self.current_anime > 0 => {
                        self.focus = FocusedElement::BackButton
                    }
                    NavDirection::Down => self.focus = FocusedElement::SyncButtons,
                    _ => {}
                }

                if nav_config.is_select(&key_event.code) {
                    self.all_selected = !self.all_selected;
                    self.change_button_lables();
                }
            }

            FocusedElement::SyncButtons => {
                // Navigation
                match nav_config.get_direction(&key_event.code) {
                    NavDirection::Left => {
                        self.nav.move_left();
                    }
                    NavDirection::Right => self.nav.move_right(),
                    NavDirection::Up => {
                        if self.current_anime > 0 {
                            self.focus = FocusedElement::BackButton;
                        } else {
                            self.focus = FocusedElement::BatchButton;
                        }
                    }
                    _ => {}
                }

                // Selection
                if nav_config.is_select(&key_event.code)
                    && let Some(anime) = self.current_anime().cloned()
                {
                    return self.handle_sync_or_delete(anime);
                }

                // Close
                if nav_config.is_close(&key_event.code) {
                    self.close();
                }
            }
        }

        None
    }

    fn is_in_rect(rect: Rect, x: u16, y: u16) -> bool {
        x >= rect.x && x < rect.x + rect.width && y >= rect.y && y < rect.y + rect.height
    }

    pub fn handle_mouse(&mut self, mouse_event: MouseEvent) -> Option<Action> {
        if !self.toggled {
            return None;
        }

        let is_click = matches!(mouse_event.kind, MouseEventKind::Down(_));
        let (x, y) = (mouse_event.column, mouse_event.row);

        // Copy areas to avoid borrow issues
        let back_area = self.back_btn_area;
        let batch_area = self.batch_btn_area;

        // Handle hover - update focus based on mouse position
        let over_back = back_area.is_some_and(|area| Self::is_in_rect(area, x, y));
        let over_batch = batch_area.is_some_and(|area| Self::is_in_rect(area, x, y));
        let over_sync = self.nav.get_hovered_index(mouse_event).is_some();

        if over_back {
            self.focus = FocusedElement::BackButton;
        } else if over_batch {
            self.focus = FocusedElement::BatchButton;
        } else if over_sync {
            self.focus = FocusedElement::SyncButtons;
        }

        // Handle clicks
        if is_click {
            if over_back {
                self.back();
                if self.current_anime == 0 {
                    self.focus = FocusedElement::SyncButtons;
                }
                return None;
            }

            if over_batch {
                self.all_selected = !self.all_selected;
                self.change_button_lables();
                return None;
            }

            if over_sync && let Some(anime) = self.current_anime().cloned() {
                return self.handle_sync_or_delete(anime);
            }
        }

        None
    }

    fn render_summary(&self, frame: &mut Frame, area: Rect) {
        let to_be_synced = self
            .to_be_processed
            .iter()
            .filter(|(sync, _)| *sync)
            .count();
        let to_be_removed = self
            .to_be_processed
            .iter()
            .filter(|(sync, _)| !*sync)
            .count();
        let already_synced = self
            .finished_processing
            .iter()
            .filter(|(sync, _)| *sync)
            .count();
        let already_removed = self
            .finished_processing
            .iter()
            .filter(|(sync, _)| !*sync)
            .count();

        let total = self.to_be_processed.len();
        let finished = self.finished_processing.len();
        let progress = if total > 0 {
            finished as f64 / total as f64
        } else {
            1.0
        };
        let is_complete = finished >= total;

        let width = std::cmp::min(area.width * 50 / 100, 50);
        let height = 12;

        let popup_area = Rect::new(
            area.x + (area.width.saturating_sub(width)) / 2,
            area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        );

        frame.render_widget(Clear, popup_area);

        let title = if is_complete {
            " Sync Complete "
        } else {
            " Syncing... "
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ratatui::symbols::border::ROUNDED)
            .title(title)
            .title_alignment(Alignment::Center)
            .style(Style::default().fg(Config::global().theme.secondary));

        frame.render_widget(block.clone(), popup_area);

        let inner = block.inner(popup_area);

        // Split into progress bar and text areas
        let [progress_area, text_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(0)])
            .areas(inner);

        // Render progress bar
        let progress_bar = Gauge::default()
            .block(Block::default().padding(Padding::horizontal(1)))
            .gauge_style(Style::default().fg(Config::global().theme.highlight))
            .ratio(progress)
            .label(format!("{}/{}", finished, total));

        frame.render_widget(progress_bar, progress_area);

        let close_text = if is_complete {
            "Press any key to close"
        } else {
            "Processing..."
        };

        let summary_text = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    "Synced to MAL: ",
                    Style::default().fg(Config::global().theme.highlight),
                ),
                Span::styled(
                    format!("{}/{}", already_synced, to_be_synced),
                    Style::default().fg(Config::global().theme.completed).bold(),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    "Removed local: ",
                    Style::default().fg(Config::global().theme.highlight),
                ),
                Span::styled(
                    format!("{}/{}", already_removed, to_be_removed),
                    Style::default().fg(Config::global().theme.error).bold(),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                close_text,
                Style::default().fg(Config::global().theme.secondary),
            )),
        ];

        let paragraph = Paragraph::new(summary_text)
            .alignment(Alignment::Center)
            .block(Block::default().padding(Padding::new(1, 1, 0, 0)));

        frame.render_widget(paragraph, text_area);
    }

    pub fn render(&mut self, frame: &mut Frame) {
        if !self.toggled {
            return;
        }

        let area = frame.area();

        // If no animes left to sync, show summary
        let anime = match self.current_anime().cloned() {
            Some(a) => a,
            None => {
                self.render_summary(frame, area);
                return;
            }
        };
        let mal_anime = self.info.anime_store.get(&anime.id);

        // Increase width slightly to accommodate two columns side-by-side
        let width = std::cmp::min(area.width * 80 / 100, 100);
        let height = std::cmp::min(area.height * 60 / 100, 25);

        let popup_area = Rect::new(
            area.x + (area.width.saturating_sub(width)) / 2,
            area.y + (area.height.saturating_sub(height)) / 2,
            width,
            height,
        );

        // self.popup_area = Some(popup_area);

        // Clear background
        frame.render_widget(Clear, popup_area);

        // Main Block
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(ratatui::symbols::border::ROUNDED)
            .title(format!(
                " Sync Conflict ({}/{}) ",
                self.to_be_processed.len(),
                self.animes_to_sync.len()
            ))
            .title_alignment(Alignment::Center)
            .style(Style::default().fg(Config::global().theme.secondary));

        frame.render_widget(block.clone(), popup_area);

        // Inner Layout: Top (Content), Bottom (Buttons)
        let inner_area = block.inner(popup_area);
        let [content_area, button_row] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),
                Constraint::Length(3), // Height for buttons
            ])
            .areas(inner_area);

        // Content Layout: Left (Image + Back), Right (Text Info)
        let [left_area, text_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Ratio(1, 4), // Image takes ~25%
                Constraint::Ratio(3, 4),
            ])
            .areas(content_area);

        // Split left area into image and back button
        let [image_area, back_btn_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(3)])
            .areas(left_area);

        // 1. Render Image
        // Adjust image area for padding
        let img_render_area = image_area.inner(Margin::new(1, 1));
        ImageManager::render_image(&self.image_manager, &anime, frame, img_render_area, true);

        // Render Back button (only if not on first anime)
        if self.current_anime > 0 {
            let back_btn_area = back_btn_area.inner(Margin::new(1, 0));
            self.back_btn_area = Some(back_btn_area);
            let back_btn = Paragraph::new("< Back").alignment(Alignment::Center).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_set(ratatui::symbols::border::ROUNDED)
                    .style(
                        Style::default().fg(if self.focus == FocusedElement::BackButton {
                            Config::global().theme.highlight
                        } else {
                            Config::global().theme.primary
                        }),
                    ),
            );
            frame.render_widget(back_btn, back_btn_area);
        } else {
            self.back_btn_area = None;
        }

        // 2. Render Text Info
        let title = if anime.alternative_titles.en.is_empty() {
            &anime.title
        } else {
            &anime.alternative_titles.en
        };

        let [title_area, text_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .areas(text_area);

        let [title_area, all_btn_area, _] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(10),
                Constraint::Length(1),
            ])
            .areas(title_area);

        let [conflict_location, text_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(0)])
            .areas(text_area);

        let [local_conflict_area, remote_conflict_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(conflict_location);

        let [local_text_area, remote_text_area] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(text_area);

        let title_text = vec![Line::from(vec![
            Span::styled(
                "Anime: ",
                Style::default().fg(Config::global().theme.secondary),
            ),
            Span::raw(title).bold(),
        ])];

        let conflict_local_text = vec![Line::from(vec![Span::styled(
            "Local save:",
            Style::default().fg(Config::global().theme.highlight),
        )])];

        let conflict_remote_text = vec![Line::from(vec![Span::styled(
            "MAL save:",
            Style::default().fg(Config::global().theme.highlight),
        )])];

        let theme = Config::global().theme.clone();

        let status_color = mal_anime
            .as_ref()
            .filter(|remote| anime.my_list_status.status != remote.my_list_status.status)
            .map(|_| theme.error)
            .unwrap_or(theme.primary);

        let score_color = mal_anime
            .as_ref()
            .filter(|remote| anime.my_list_status.score != remote.my_list_status.score)
            .map(|_| theme.error)
            .unwrap_or(theme.primary);

        let watched_color = mal_anime
            .as_ref()
            .filter(|remote| {
                anime.my_list_status.num_episodes_watched
                    != remote.my_list_status.num_episodes_watched
            })
            .map(|_| theme.error)
            .unwrap_or(theme.primary);

        let local_info_text = vec![
            Line::from(vec![
                Span::raw("Status: "),
                Span::styled(
                    anime.my_list_status.status.to_string(),
                    Style::default().fg(status_color),
                ),
            ]),
            Line::from(vec![
                Span::raw("Score: "),
                Span::styled(
                    anime.my_list_status.score.to_string(),
                    Style::default().fg(score_color),
                ),
            ]),
            Line::from(vec![
                Span::raw("Watched: "),
                Span::styled(
                    format!(
                        "{}/{}",
                        anime.my_list_status.num_episodes_watched, anime.num_episodes
                    ),
                    Style::default().fg(watched_color),
                ),
            ]),
        ];

        if mal_anime.is_none() {
            // No remote info available
            let no_remote_info = vec![Line::from(vec![Span::styled(
                "Not added to list.",
                Style::default().fg(Config::global().theme.error),
            )])];

            let remote_paragraph = Paragraph::new(no_remote_info)
                .wrap(Wrap { trim: true })
                .block(Block::default().padding(Padding::new(1, 1, 1, 1)));
            frame.render_widget(remote_paragraph, remote_text_area);
        } else {
            let remote_info_text = match mal_anime.as_ref() {
                None => vec![Line::from(vec![Span::styled(
                    "Not added to list.",
                    Style::default().fg(Config::global().theme.error),
                )])],
                Some(remote) => vec![
                    Line::from(vec![
                        Span::raw("Status: "),
                        Span::styled(
                            remote.my_list_status.status.to_string(),
                            Style::default().fg(status_color),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("Score: "),
                        Span::styled(
                            remote.my_list_status.score.to_string(),
                            Style::default().fg(score_color),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("Watched: "),
                        Span::styled(
                            format!(
                                "{}/{}",
                                remote.my_list_status.num_episodes_watched, remote.num_episodes
                            ),
                            Style::default().fg(watched_color),
                        ),
                    ]),
                ],
            };

            let remote_paragraph = Paragraph::new(remote_info_text)
                .wrap(Wrap { trim: true })
                .block(Block::default().padding(Padding::new(1, 1, 1, 1)));

            frame.render_widget(remote_paragraph, remote_text_area);
        }

        let title_paragraph = Paragraph::new(title_text)
            .alignment(Alignment::Left)
            .block(Block::default().padding(Padding::new(1, 1, 1, 0)));
        frame.render_widget(title_paragraph, title_area);

        let conflict_location_paragraph = Paragraph::new(conflict_local_text)
            .alignment(Alignment::Left)
            .block(Block::default().padding(Padding::new(1, 1, 1, 0)));
        frame.render_widget(conflict_location_paragraph, local_conflict_area);

        let conflict_remote_paragraph = Paragraph::new(conflict_remote_text)
            .alignment(Alignment::Left)
            .block(Block::default().padding(Padding::new(1, 1, 1, 0)));
        frame.render_widget(conflict_remote_paragraph, remote_conflict_area);

        let local_paragraph = Paragraph::new(local_info_text)
            .wrap(Wrap { trim: true })
            .block(Block::default().padding(Padding::new(1, 1, 1, 1)));
        frame.render_widget(local_paragraph, local_text_area);

        // 3. Render Buttons
        // We want the buttons centered or spread out.
        // Let's use the Navigatable construct logic you already have.
        self.batch_btn_area = Some(all_btn_area);
        let all_btn = if self.all_selected {
            "plural"
        } else {
            "single"
        };
        let all_btn = Paragraph::new(all_btn).alignment(Alignment::Center).block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(ratatui::symbols::border::ROUNDED)
                .style(
                    Style::default().fg(if self.focus == FocusedElement::BatchButton {
                        Config::global().theme.highlight
                    } else {
                        Config::global().theme.primary
                    }),
                ),
        );
        frame.render_widget(all_btn, all_btn_area);

        let buttons_rect = Rect::new(
            button_row.x + 1,
            button_row.y,
            button_row.width.saturating_sub(2),
            button_row.height,
        );

        // Create button labels with " (all)" appended if ctrl is held
        self.nav.construct(
            &self.buttons,
            buttons_rect,
            |button_label, area, highlighted| {
                let style = if highlighted && self.focus == FocusedElement::SyncButtons {
                    Style::default().fg(Config::global().theme.highlight)
                } else {
                    Style::default().fg(Config::global().theme.primary)
                };

                let btn_text = Paragraph::new(button_label.as_str())
                    .alignment(Alignment::Center)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .border_set(ratatui::symbols::border::ROUNDED)
                            .style(style),
                    );

                frame.render_widget(btn_text, area);
            },
        );
    }
}
