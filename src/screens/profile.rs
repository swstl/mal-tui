use std::sync::Arc;
use std::sync::Mutex;
use std::thread::JoinHandle;

use crate::add_screen_caching;
use crate::app::Action;
use crate::app::Event;
use crate::check_for_account;
use crate::config::Config;
use crate::config::navigation::NavDirection;
use crate::mal::models::anime::Anime;
use crate::mal::models::anime::FavoriteAnime;
use crate::mal::models::user::User;
use crate::utils::functionStreaming::StreamableRunner;
use crate::utils::imageManager::ImageManager;
use crate::utils::stringManipulation::format_date;
use crate::utils::terminalCapabilities::TERMINAL_RATIO;

use super::BackgroundUpdate;
use super::ExtraInfo;
use super::Screen;
use super::widgets::navigatable::Navigatable;

use crossterm::event::KeyEvent;
use ratatui::Frame;
use ratatui::layout::Alignment;
use ratatui::layout::Constraint;
use ratatui::layout::Direction;
use ratatui::layout::Layout;
use ratatui::layout::Margin;
use ratatui::layout::Rect;
use ratatui::style;
use ratatui::style::Style;
use ratatui::symbols;
use ratatui::widgets;
use ratatui::widgets::Block;
use ratatui::widgets::Borders;
use ratatui::widgets::Gauge;
use ratatui::widgets::Paragraph;

const PICTURE_RATIO: f32 = 225.0 / 320.0;
const PFP_RATIO: f32 = 225.0 / 280.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    NavBar,
    Content,
}

#[derive(Clone)]
pub struct ProfileScreen {
    user: User,
    focus: Focus,
    image_manager: Arc<Mutex<ImageManager>>,
    bg_loaded: bool,
    app_info: ExtraInfo,
    navigation_fav: Navigatable,
}
impl ProfileScreen {
    pub fn new(info: ExtraInfo) -> Self {
        Self {
            focus: Focus::NavBar,
            image_manager: Arc::new(Mutex::new(ImageManager::new())),
            bg_loaded: false,
            user: User::empty(),
            app_info: info,
            navigation_fav: Navigatable::new((2, 5)),
        }
    }
}

impl Screen for ProfileScreen {
    add_screen_caching!();
    check_for_account!();

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(widgets::Clear, area);

        // just the navbar bro
        let [_, bottom] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Percentage(100)])
            .areas(area);

        // the actual screen
        let [left, right] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(26), Constraint::Fill(1)])
            .areas(bottom);

        let pfp_height = ((left.width as f32 * PFP_RATIO) / TERMINAL_RATIO) as u16;

        //pfp
        let [pfp, info] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(pfp_height), Constraint::Fill(1)])
            .areas(left);

        ImageManager::render_image(
            &self.image_manager,
            &self.user,
            frame,
            pfp.inner(Margin::new(1, 1)),
            false,
        );

        // favorite animes
        let [right_top, fav_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1), Constraint::Percentage(40)])
            .areas(right);
        let block = Block::default()
            .border_set(symbols::border::ROUNDED)
            .borders(Borders::ALL)
            .title("Favorited Animes")
            .border_style(style::Style::default().fg(Config::global().theme.primary));
        frame.render_widget(block, fav_area);

        self.navigation_fav.construct(
            &self.user.favorited_animes,
            fav_area.inner(Margin::new(1, 1)),
            |anime, area, selected| {
                if selected && self.focus == Focus::Content {
                    frame.render_widget(
                        Block::default()
                            .border_set(symbols::border::ROUNDED)
                            .borders(Borders::ALL)
                            .border_style(
                                style::Style::default().fg(Config::global().theme.highlight),
                            ),
                        area,
                    );
                }

                let [title, image] = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Fill(1)])
                    .areas(area.inner(Margin::new(1, 1)));

                // favorite anime title
                let title_text = Paragraph::new(anime.title.clone()).alignment(Alignment::Center);
                frame.render_widget(title_text, title);

                // favorite anime image
                let image_width = (image.height as f32 * PICTURE_RATIO * TERMINAL_RATIO) as u16;
                let centered_image_area = Rect::new(
                    image.x + (image.width - image_width) / 2,
                    image.y,
                    image_width,
                    image.height,
                );

                ImageManager::render_image(
                    &self.image_manager,
                    anime,
                    frame,
                    centered_image_area,
                    selected,
                );
            },
        );

        // user information right side
        let block = Block::default()
            .border_set(symbols::border::ROUNDED)
            .borders(Borders::ALL)
            .title("User Statistics")
            .border_style(style::Style::default().fg(Config::global().theme.primary));
        frame.render_widget(block, right_top);

        // user information left side

        let user_info_block = Block::default()
            .borders(Borders::ALL)
            .border_set(symbols::border::ROUNDED)
            .title("User Info")
            .border_style(style::Style::default().fg(Config::global().theme.primary));
        frame.render_widget(user_info_block, info);

        let user_info_text = [
            format!("Username: {}", self.user.name),
            format!("Joined: {}", format_date(&self.user.joined_at)),
            format!("Gender: {}", self.user.gender),
            format!("Birthday: {}", format_date(&self.user.birthday)),
            format!("Location: {}", self.user.location),
            format!("Time Zone: {}", self.user.time_zone),
        ];
        for (i, line) in user_info_text.iter().enumerate() {
            let paragraph = Paragraph::new(line.clone())
                .alignment(Alignment::Left)
                .block(Block::default().borders(Borders::NONE));
            frame.render_widget(
                paragraph,
                Rect::new(info.x + 1, info.y + 1 + i as u16, info.width, 1),
            );
        }

        // anime watch percentages
        if self.user.anime_statistics.num_items == 0 {
            let area = Rect::new(
                right_top.x,
                right_top.y + right_top.height / 2,
                right_top.width,
                1,
            );
            let no_data_text =
                Paragraph::new("No animes watched yet!").alignment(Alignment::Center);
            frame.render_widget(no_data_text, area);
            return;
        }

        let percentages = [
            self.user.anime_statistics.num_items_watching,
            self.user.anime_statistics.num_items_completed,
            self.user.anime_statistics.num_items_on_hold,
            self.user.anime_statistics.num_items_dropped,
            self.user.anime_statistics.num_items_plan_to_watch,
        ];

        let [percentage_area, _right_middle_side, _right_right_side] = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(33),
                Constraint::Percentage(33),
                Constraint::Fill(1),
            ])
            .areas(right_top.inner(Margin::new(4, 2)));

        let [subtitle, percentage_area] = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Fill(1)])
            .areas(percentage_area);

        let subtitle_text = Paragraph::new("Overall Anime Statistics")
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::NONE));
        frame.render_widget(subtitle_text, subtitle);

        for (i, &count) in percentages.iter().enumerate() {
            let percentage =
                (count as f32 / self.user.anime_statistics.num_items as f32 * 100.0) as u16;
            let label = match i {
                0 => "Watching",
                1 => "Completed",
                2 => "On Hold",
                3 => "Dropped",
                4 => "Plan to Watch",
                _ => "",
            };

            let area = Rect::new(
                percentage_area.x + 1,
                percentage_area.y + i as u16 * 3,
                percentage_area.width - 2,
                3,
            );

            let gauge = Gauge::default()
                .gauge_style(
                    Style::new()
                        .fg(Config::global().theme.status_color(label))
                        .bg(style::Color::Black),
                )
                .percent(percentage);

            let label_text = Paragraph::new(label)
                .alignment(Alignment::Left)
                .block(Block::default().borders(Borders::NONE));
            let label_value = Paragraph::new(format!("{}", percentages[i]))
                .alignment(Alignment::Right)
                .block(Block::default().borders(Borders::NONE));

            frame.render_widget(gauge, area.inner(Margin::new(0, 1)));
            frame.render_widget(label_text, area);
            frame.render_widget(label_value, area);
        }

        let total_animes_text = Paragraph::new(concat!(
            "Total Animes:\n",
            "Total Episodes:\n",
            "Total Days Watched:\n",
            "Mean Score:\n",
            "Total Scores Given:"
        ))
        .alignment(Alignment::Left);

        let total_animes_value = Paragraph::new(format!(
            "{}\n{}\n{}\n{}\n{}",
            self.user.anime_statistics.num_items,
            self.user.anime_statistics.num_episodes,
            self.user.anime_statistics.num_days,
            self.user.anime_statistics.mean_score,
            self.user.user_stats.num_items_rated
        ))
        .alignment(Alignment::Right);

        let total_area = Rect::new(
            percentage_area.x + 1,
            percentage_area.y + percentages.len() as u16 * 3 + 1,
            percentage_area.width - 2,
            5,
        );

        frame.render_widget(total_animes_text, total_area);
        frame.render_widget(total_animes_value, total_area);
    }

    // handle inut function
    // for this spcific screen bro
    fn handle_keyboard(&mut self, key_event: KeyEvent) -> Option<Action> {
        let modifier = key_event
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);
        let nav = &Config::global().navigation;

        match self.focus {
            Focus::NavBar => {
                self.focus = Focus::Content;
            }
            Focus::Content => {
                if modifier && nav.get_direction(&key_event.code) == NavDirection::Up {
                    self.focus = Focus::NavBar;
                    return Some(Action::NavbarSelect(true));
                }

                match nav.get_direction(&key_event.code) {
                    NavDirection::Down => {
                        self.navigation_fav.move_down();
                    }
                    NavDirection::Up => {
                        self.navigation_fav.move_up();
                    }
                    NavDirection::Left => {
                        self.navigation_fav.move_left();
                    }
                    NavDirection::Right => {
                        self.navigation_fav.move_right();
                    }
                    _ => {}
                }

                if nav.is_select(&key_event.code)
                    && let Some(anime) = self
                        .navigation_fav
                        .get_selected_item_mut(&mut self.user.favorited_animes)
                {
                    return Some(Action::ShowOverlay(anime.id));
                }
            }
        }

        None
    }

    fn handle_mouse(&mut self, mouse_event: crossterm::event::MouseEvent) -> Option<Action> {
        if mouse_event.row < 3 {
            self.focus = Focus::NavBar;
            return Some(Action::NavbarSelect(true));
        }

        if self.navigation_fav.get_hovered_index(mouse_event).is_some() {
            self.focus = Focus::Content;

            if let crossterm::event::MouseEventKind::Down(_) = mouse_event.kind {
                let anime_id = self
                    .navigation_fav
                    .get_selected_item(&self.user.favorited_animes)?;
                return Some(Action::ShowOverlay(anime_id.id));
            }
        }

        None
    }

    fn background(&mut self) -> Option<JoinHandle<()>> {
        if self.bg_loaded {
            return None;
        }
        self.bg_loaded = true;

        let info = self.app_info.clone();
        let image_manager = self.image_manager.clone();
        let id = self.get_name();
        ImageManager::init_with_threads(&self.image_manager, info.app_sx.clone());

        Some(std::thread::spawn(move || {
            // get the users information
            if let Some(user) = info.mal_client.get_user() {
                let username = user.name.clone();
                ImageManager::query_image_for_fetching(&image_manager, &user);
                let update = BackgroundUpdate::new(id.clone()).set("user", user);
                info.app_sx.send(Event::BackgroundNotice(update)).ok();

                // get the users anime list
                let anime_generator = StreamableRunner::new()
                    .with_batch_size(1000)
                    .stop_early()
                    .stop_at(20); // just a limit in case (20 x batch size)

                for animes in anime_generator
                    .run(|offset, limit| info.mal_client.get_anime_list(None, offset, limit))
                {
                    // for the favorited animes to be clickable we need the anime details (now
                    // most likely the favorite anime is in the user list so we just fetch that,
                    // which we do anyways but now we just copy it with "animes", anime.clone())
                    let update = BackgroundUpdate::new(id.clone())
                        .set("animes", animes.clone())
                        .set("listed_animes", animes);
                    info.app_sx.send(Event::BackgroundNotice(update)).ok();
                }

                // get the users favorited animes
                if let Some(favorited_animes) = info.mal_client.get_favorited_anime(username) {
                    for anime in favorited_animes.clone() {
                        ImageManager::query_image_for_fetching(&image_manager, &anime);
                    }
                    let update =
                        BackgroundUpdate::new(id.clone()).set("favorited_animes", favorited_animes);
                    info.app_sx.send(Event::BackgroundNotice(update)).ok();
                }
            }
        }))
    }

    fn apply_update(&mut self, mut update: BackgroundUpdate) {
        if let Some(user) = update.take::<User>("user") {
            self.user = user;
        }
        if let Some(favorited_animes) = update.take::<Vec<FavoriteAnime>>("favorited_animes") {
            self.user.add_favorite_animes(favorited_animes);
        }
        if let Some(animes) = update.take::<Vec<Anime>>("listed_animes") {
            self.user.add_listed_animes(animes);
        }
    }
}
