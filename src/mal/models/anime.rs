use crate::utils::imageManager::HasDisplayableImage;
use crate::utils::store::Storable;
use crate::mal::network::fetch_favorited_anime;
use crate::mal::network::fetch_anime;
use crate::mal::network::Identifier;
use crate::mal::network::Update;
use crate::mal::Fetchable;
use super::na;


use database::*;
use serde::Deserializer;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::{self};
use chrono::Utc;

// season limit (first season ever) : year: 1917 season: winter

pub type AnimeId = <Anime as Storable>::Id;


/// Converts anime airing status from API format to internal format
pub fn anime_status_from_api(s: String) -> String {
    match s.as_str() {
        "currently_airing" => "airing".to_string(),
        "finished_airing" => "finished".to_string(),
        "not_yet_aired" => "upcoming".to_string(),
        _ => s,
    }
}

/// Converts user watch status from API format to internal format
pub fn watch_status_from_api(s: String) -> String {
    match s.as_str() {
        "add_to_list" => "".to_string(),
        "on_hold" => "on hold".to_string(),
        "plan_to_watch" => "plan to watch".to_string(),
        _ => s,
    }
}

/// Converts user watch status from internal format to API format
pub fn watch_status_to_api(s: String) -> String {
    match s.as_str() {
        "on hold" | "on-hold" => "on_hold".to_string(),
        "plan to watch" => "plan_to_watch".to_string(),
        _ => s,
    }
}

pub fn status_is_known(s: String) -> bool {
    matches!(
        s.as_str(),
        "watching" | "completed" | "on hold" | "on-hold" | "dropped" | "plan to watch" | "on_hold" | "plan_to_watch"
    )
}

// Deserializers (thin wrappers around the transformation functions)
fn anime_status<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(anime_status_from_api(s))
}

fn list_status<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(watch_status_from_api(s))
}

// MAL's `statistics` object returns its status counts as JSON strings
// (e.g. "watching": "238321") even though they are integers. Accept either a
// string or a number so single-anime detail responses deserialize cleanly.
fn de_u64_flexible<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StrOrNum {
        Str(String),
        Num(u64),
    }
    match StrOrNum::deserialize(deserializer)? {
        StrOrNum::Str(s) => s.trim().parse::<u64>().map_err(serde::de::Error::custom),
        StrOrNum::Num(n) => Ok(n),
    }
}

fn default_true() -> bool {
    true
}

#[allow(unused)]
pub mod fields {
    pub const ID: &str = "id";
    pub const TITLE: &str = "title";
    pub const MAIN_PICTURE: &str = "main_picture";
    pub const ALTERNATIVE_TITLES: &str = "alternative_titles";
    pub const START_DATE: &str = "start_date";
    pub const END_DATE: &str = "end_date";
    pub const SYNOPSIS: &str = "synopsis";
    pub const MEAN: &str = "mean";
    pub const RANK: &str = "rank";
    pub const POPULARITY: &str = "popularity";
    pub const NUM_LIST_USERS: &str = "num_list_users";
    pub const NUM_SCORING_USERS: &str = "num_scoring_users";
    pub const NSFW: &str = "nsfw";
    pub const CREATED_AT: &str = "created_at";
    pub const UPDATED_AT: &str = "updated_at";
    pub const MEDIA_TYPE: &str = "media_type";
    pub const STATUS: &str = "status";
    pub const GENRES: &str = "genres";
    pub const MY_LIST_STATUS: &str = "my_list_status";
    pub const NUM_EPISODES: &str = "num_episodes";
    pub const START_SEASON: &str = "start_season";
    pub const BROADCAST: &str = "broadcast";
    pub const SOURCE: &str = "source";
    pub const AVERAGE_EPISODE_DURATION: &str = "average_episode_duration";
    pub const RATING: &str = "rating";
    pub const PICTURES: &str = "pictures";
    pub const BACKGROUND: &str = "background";
    pub const RELATED_ANIME: &str = "related_anime";
    pub const RELATED_MANGA: &str = "related_manga";
    pub const RECOMMENDATIONS: &str = "recommendations";
    pub const STUDIOS: &str = "studios";
    pub const STATISTICS: &str = "statistics";
    pub const ALL: [&str; 32] = [
        AVERAGE_EPISODE_DURATION,
        ALTERNATIVE_TITLES,
        NUM_SCORING_USERS,
        RECOMMENDATIONS,
        NUM_LIST_USERS,
        MY_LIST_STATUS,
        RELATED_MANGA,
        RELATED_ANIME,
        MAIN_PICTURE,
        START_SEASON,
        NUM_EPISODES,
        STATISTICS,
        START_DATE,
        MEDIA_TYPE,
        UPDATED_AT,
        POPULARITY,
        BACKGROUND,
        CREATED_AT,
        BROADCAST,
        SYNOPSIS,
        PICTURES,
        END_DATE,
        STUDIOS,
        GENRES,
        RATING,
        SOURCE,
        STATUS,
        TITLE,
        NSFW,
        RANK,
        MEAN,
        ID,
    ];
}

/// Anime model representing the structure of an anime object
///
/// # Fields
/// - `id` - Unique identifier
/// - `title` - Main title
/// - `main_picture` - Cover image
/// - `alternative_titles` - Titles in different languages
/// - `start_date` / `end_date` - Airing period
/// - `synopsis` - Plot summary
/// - `mean` - Average rating (0.0-10.0)
/// - `rank` - Ranking position
/// - `popularity` - Popularity ranking
/// - `num_list_users` - Users with this in their list
/// - `num_scoring_users` - Users who scored this
/// - `nsfw` - Content rating
/// - `created_at` / `updated_at` - Timestamps
/// - `media_type` - Type (tv, movie, ova, etc.)
/// - `status` - Airing status
/// - `genres` - Genre categories
/// - `my_list_status` - User's personal status
/// - `num_episodes` - Episode count
/// - `start_season` - Season and year
/// - `broadcast` - Broadcasting schedule
/// - `source` - Source material
/// - `average_episode_duration` - Episode length
/// - `rating` - Content rating
/// - `pictures` - Additional images
/// - `background` - Production info
/// - `related_anime` / `related_manga` - Related content
/// - `recommendations` - Similar anime
/// - `studios` - Animation studios
/// - `statistics` - User interaction stats
#[allow(unused)]
#[derive(Debug, Clone, Deserialize, Serialize, Entry)]
pub struct Anime {
    /// Unique identifier for the anime
    #[primary_key]
    #[serde(default)]
    pub id: usize,

    /// Title of the anime
    #[serde(default = "na")]
    pub title: String,

    /// Main picture of the anime { large, medium }
    #[serde(default)]
    pub main_picture: Pictures,

    /// Alternative titles for the anime { synonyms, en, ja }
    #[serde(default)]
    pub alternative_titles: AlternativeTitles,

    /// Start date of the anime in YYYY-MM-DD format
    #[serde(default = "na")]
    pub start_date: String,

    /// End date of the anime in YYYY-MM-DD format
    #[serde(default = "na")]
    pub end_date: String,

    /// Synopsis of the anime
    #[serde(default = "na")]
    pub synopsis: String,

    /// Mean score of the anime
    #[serde(default)]
    pub mean: f32,

    /// Rank of the anime
    #[serde(default)]
    pub rank: u64,

    /// Popularity score of the anime - lower is more popular
    #[serde(default)]
    pub popularity: u64,

    /// Number of users who have added this anime to their list
    #[serde(default)]
    pub num_list_users: u64,

    /// Number of users who have scored this anime
    #[serde(default)]
    pub num_scoring_users: u64,

    /// NSFW (Not Safe For Work) status of the anime
    #[serde(default = "na")]
    pub nsfw: String,

    /// Creation date of the anime entry in ISO 8601 format
    #[serde(default = "na")]
    pub created_at: String,

    /// Last updated date of the anime entry in ISO 8601 format
    #[serde(default = "na")]
    pub updated_at: String,

    /// Media type of the anime (e.g., TV, Movie, OVA)
    #[serde(default = "na")]
    pub media_type: String,

    /// Status of the anime (e.g., airing, finished, upcoming)
    #[serde(deserialize_with = "anime_status", default = "na")]
    pub status: String,

    /// Genres associated with the anime
    #[serde(default)]
    pub genres: Vec<Genre>,

    /// User's personal MyAnimeList status for this anime
    ///
    /// # Fields
    /// - `status` - Watch status (watching/completed/on_hold/dropped/plan_to_watch)
    /// - `score` - User rating (0-10)
    /// - `num_episodes_watched` - Current progress
    /// - `is_rewatching` - Rewatch flag
    /// - `start_date`/`finish_date` - Watch period
    /// - `priority` - User priority (0-2)
    /// - `num_times_rewatched`/`rewatch_value` - Rewatch statistics
    /// - `tags`/`comments` - User notes
    /// - `updated_at` - Last modified
    #[serde(default)]
    pub my_list_status: MyListStatus,

    /// Number of episodes in the anime
    #[serde(default)]
    pub num_episodes: u32,

    /// Number of episodes that have been released
    #[serde(default)]
    pub num_released_episodes: Option<u32>,

    /// Start season of the anime { year, season }
    #[serde(default)]
    pub start_season: StartSeason,

    /// Broadcast information of the anime { day_of_the_week, start_time }
    pub broadcast: Option<Broadcast>,

    /// Source material of the anime (e.g., manga, light novel)
    #[serde(default = "na")]
    pub source: String,

    /// Average duration of an episode in seconds
    #[serde(default)]
    pub average_episode_duration: u64,

    /// Rating of the anime (e.g., PG-13, R)
    #[serde(default = "na")]
    pub rating: String,

    /// Pictures associated with the anime
    pub pictures: Option<Vec<Pictures>>,

    /// Background image or description of the anime
    #[serde(default = "na")]
    pub background: String,

    /// Related anime { node, relation_type, relation_type_formatted }
    pub related_anime: Option<Vec<RelatedAnime>>,

    /// Related manga { node, relation_type, relation_type_formatted }
    pub related_manga: Option<Vec<RelatedManga>>,

    /// Recommendations for the anime { node, num_recommendations }
    pub recommendations: Option<Vec<Recommendation>>,

    /// Studios that produced the anime
    #[serde(default)]
    pub studios: Vec<Studio>,

    /// Statistics about the anime { status, num_list_users }
    #[serde(default)]
    pub statistics: Statistics,

    /// If the number of episodes is decided  
    #[serde(default = "default_true")]
    pub episode_count_ready: bool,

    /// When the local db row last took status and score from the api (local only).
    /// Must stay the last field: rows are read by column index, and the
    /// migration in MalClient::new appends this column at the end
    #[serde(default)]
    pub refreshed_at: Option<String>,
}

impl Anime {
    pub fn empty() -> Self {
        Self {
            id: 0,
            title: String::new(),
            main_picture: Pictures::default(),
            alternative_titles: AlternativeTitles::default(),
            start_date: String::new(),
            end_date: String::new(),
            synopsis: String::new(),
            mean: 0.0,
            rank: 0,
            popularity: 0,
            num_list_users: 0,
            num_scoring_users: 0,
            nsfw: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            media_type: String::new(),
            status: String::new(),
            genres: Vec::new(),
            my_list_status: MyListStatus::default(),
            num_episodes: 0,
            num_released_episodes: None,
            start_season: StartSeason::default(),
            broadcast: None,
            source: String::new(),
            average_episode_duration: 0,
            rating: String::new(),
            pictures: None,
            background: String::new(),
            related_anime: None,
            related_manga: None,
            recommendations: None,
            studios: Vec::new(),
            statistics: Statistics::default(),
            episode_count_ready: false,
            refreshed_at: None,
        }
    }

    #[allow(dead_code)]
    pub fn example(id: usize) -> Self {
        Self {
            id,
            title: "Sono Bisque Doll wa Koi wo Suru Season 2".to_string(),
            main_picture: Pictures {
                large: "https://cdn.myanimelist.net/images/anime/1712/148299l.jpg".to_string(),
                medium: "https://cdn.myanimelist.net/images/anime/1526/148873.jpg".to_string(),
            },
            alternative_titles: AlternativeTitles {
                synonyms: vec!["Synonym 1".to_string(), "Synonym 2".to_string()],
                en: format!("Example Anime EN {}", id),
                ja: format!("Example Anime JA {}", id),
            },
            start_date: "2023-01-01".to_string(),
            end_date: "2023-12-31".to_string(),
            synopsis: "This is an example anime synopsis.".to_string(),
            mean: 8.5,
            rank: 1,
            popularity: 1000,
            num_list_users: 5000,
            num_scoring_users: 3000,
            nsfw: "safe".to_string(),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            updated_at: "2023-01-02T00:00:00Z".to_string(),
            media_type: "TV".to_string(),
            status: "finished".to_string(),
            genres: vec![Genre {
                id: 1,
                name: "Action".to_string(),
            }],
            my_list_status: MyListStatus::default(),
            num_episodes: 12,
            num_released_episodes: Some(12),
            start_season: StartSeason {
                year: 2023,
                season: "Winter".to_string(),
            },
            broadcast: Some(Broadcast {
                day_of_the_week: "Monday".to_string(),
                start_time: "18:00".to_string(),
            }),
            source: "Manga".to_string(),
            average_episode_duration: 24,
            rating: "PG-13".to_string(),
            pictures: Some(vec![Pictures::default()]),
            background: "Background image URL or description.".to_string(),
            related_anime: None,
            related_manga: None,
            recommendations: None,
            studios: vec![Studio {
                id: 1,
                name: "Studio Example".to_string(),
            }],
            statistics: Statistics::default(),
            episode_count_ready: true,
            refreshed_at: None,
        }
    }

    pub fn from_response(response: AnimeResponse) -> Vec<Self> {
        response
            .data
            .into_iter()
            .map(|anime_node| anime_node.node)
            .collect()
    }
}

impl Default for Anime {
    fn default() -> Self {
        Anime::empty()
    }
}

impl Anime {
    pub fn studios_as_string(&self) -> String {
        self.studios
            .iter()
            .map(|s| s.name.clone())
            .collect::<Vec<String>>()
            .join(", ")
    }

    /// The title to display, honoring the user's `prefer_romaji` config.
    ///
    /// MAL's main `title` field is the romaji title; `alternative_titles.en`
    /// is the English title (sent as `""`, or absent → "N/A"). When romaji is
    /// preferred we use `title`; otherwise we prefer English and fall back to
    /// romaji when no English title is available.
    pub fn display_title(&self) -> String {
        let en = &self.alternative_titles.en;
        let en_available = !en.is_empty() && en != "N/A";

        if crate::config::Config::global().prefer_romaji() {
            if !self.title.is_empty() {
                self.title.clone()
            } else {
                en.clone()
            }
        } else if en_available {
            en.clone()
        } else {
            self.title.clone()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Page {
    #[serde(default = "na")]
    pub previous: String,
    #[serde(default = "na")]
    pub next: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnimeResponse {
    pub data: Vec<AnimeNode>,
    pub paging: Option<Page>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AnimeNode {
    #[serde(default)]
    pub node: Anime,
    #[serde(default)]
    pub ranking: Ranking,
    pub list_status: Option<MyListStatus>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Ranking {
    rank: u16,
    previous_rank: Option<u16>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Pictures {
    #[serde(default = "na")]
    pub large: String,
    #[serde(default = "na")]
    pub medium: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct AlternativeTitles {
    #[serde(default)]
    pub synonyms: Vec<String>,
    #[serde(default = "na")]
    pub en: String,
    #[serde(default = "na")]
    pub ja: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Genre {
    pub id: u64,
    pub name: String,
}

impl fmt::Display for Genre {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct MyListStatus {
    #[serde(deserialize_with = "list_status", default = "na")]
    pub status: String,
    pub score: u8,
    #[serde(default)]
    pub num_episodes_watched: u32,
    pub is_rewatching: Option<bool>,
    #[serde(default = "na")]
    pub start_date: String,
    #[serde(default = "na")]
    pub finish_date: String,
    #[serde(default)]
    pub priority: u8,
    pub num_times_rewatched: Option<u8>,
    pub rewatch_value: Option<u8>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "na")]
    pub comments: String,
    #[serde(default = "na")]
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct StartSeason {
    #[serde(default)]
    pub year: u16,
    #[serde(default = "na")]
    pub season: String,
}

impl fmt::Display for StartSeason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.year == 0 && !self.season.is_empty() {
            write!(f, "{}", self.season)
        } else if self.season.is_empty() && self.year != 0 {
            write!(f, "{}", self.year)
        } else if self.year == 0 && self.season.is_empty() {
            write!(f, "N/A")
        } else {
            write!(f, "{} {}", self.season, self.year)
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Broadcast {
    #[serde(default = "na")]
    pub day_of_the_week: String,
    #[serde(default = "na")]
    pub start_time: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Studio {
    pub id: u64,
    pub name: String,
}

impl fmt::Display for Studio {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RelatedAnime {
    pub node: Node,
    #[serde(default = "na")]
    pub relation_type: String,
    #[serde(default = "na")]
    pub relation_type_formatted: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Recommendation {
    pub node: Node,
    pub num_recommendations: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Status {
    #[serde(deserialize_with = "de_u64_flexible", default)]
    pub watching: u64,
    #[serde(deserialize_with = "de_u64_flexible", default)]
    pub completed: u64,
    #[serde(deserialize_with = "de_u64_flexible", default)]
    pub on_hold: u64,
    #[serde(deserialize_with = "de_u64_flexible", default)]
    pub dropped: u64,
    #[serde(deserialize_with = "de_u64_flexible", default)]
    pub plan_to_watch: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Statistics {
    pub status: Status,
    #[serde(deserialize_with = "de_u64_flexible", default)]
    pub num_list_users: u64,
}
impl Default for Statistics {
    fn default() -> Self {
        Statistics {
            status: Status {
                watching: 0,
                completed: 0,
                on_hold: 0,
                dropped: 0,
                plan_to_watch: 0,
            },
            num_list_users: 0,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RelatedManga {
    // TODO: related manga when adding manga
    pub node: Node,
    pub relation_type: String,
    pub relation_type_formatted: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Node {
    pub id: u64,
    pub title: String,
    pub main_picture: Option<Pictures>,
}

impl Fetchable for Anime {
    type Response = AnimeResponse;
    type Output = Vec<Self>;

    fn fetch(
        identifier: Identifier,
        url: String,
        parameters: Vec<(String, String)>,
    ) -> Result<Self::Response, Box<dyn std::error::Error>> {
        fetch_anime(identifier, url, parameters)
    }

    fn from_response(response: Self::Response) -> Self::Output {
        Self::from_response(response)
    }

    fn after_fetch(animes: &Self::Output, database: &DatabaseManager) {
        Anime::refresh_stored(animes, database);
    }
}

impl Anime {
    // fresh api data wins. the user's own list entry only when mal's is newer:
    // logged out the api sends none, and a local edit can be newer than mal's.
    // both stamps are utc iso strings, so plain string comparison orders them
    pub fn merge_fresh(&mut self, mut fresh: Anime) {
        if fresh.my_list_status.updated_at <= self.my_list_status.updated_at {
            fresh.my_list_status = std::mem::take(&mut self.my_list_status);
        }
        *self = fresh;
    }

    // anime already kept in the local db take the fresh api data. every column but
    // id and my_list_status is written blindly: fields a response leaves out are
    // never shown from the db, and a reset num_released_episodes just makes the
    // popup scrape the episode count again
    pub fn refresh_stored(animes: &[Anime], database: &DatabaseManager) {
        let now = Some(Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());
        let animes: Vec<Anime> = animes
            .iter()
            .map(|anime| Anime { refreshed_at: now.clone(), ..anime.clone() })
            .collect();
        let columns: Vec<&str> = Anime::example(0)
            .bind_values()
            .into_iter()
            .map(|(name, _)| name)
            .filter(|name| !["id", "my_list_status"].contains(name))
            .collect();
        if let Err(e) = database.update_columns(&animes, &columns) {
            crate::utils::errorBus::log_to_file(&format!("Failed to refresh stored anime: {}", e));
        }
    }
}

impl fmt::Display for Anime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.title)
    }
}

impl HasDisplayableImage for Anime {
    fn get_displayable_image(&self) -> Option<(usize, String)> {
        Some((self.id, self.main_picture.large.clone()))
    }
}

fn extract_image_url<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct Images {
        jpg: ImageUrls,
    }

    #[derive(Deserialize)]
    struct ImageUrls {
        image_url: String,
    }

    let images = Images::deserialize(deserializer)?;
    Ok(images.jpg.image_url)
}

/// mini version of anime model for favorties
//
// Favorites are scraped from the public MAL profile page (see
// `network::fetch_favorited_anime`); these structs are just the carrier the
// `Fetchable` impl returns. `data`/`anime` default to empty so an absent or
// unparseable favorites section degrades to "no favorites" rather than erroring.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct FavoriteResponse {
    #[serde(default)]
    pub data: JikanData,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct JikanData {
    #[serde(default)]
    pub anime: Vec<FavoriteAnime>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FavoriteAnime {
    #[serde(alias = "mal_id")]
    #[serde(default)]
    pub id: usize,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    #[serde(deserialize_with = "extract_image_url")]
    #[serde(alias = "images")]
    pub image: String,
}

impl Fetchable for FavoriteAnime {
    type Response = FavoriteResponse;
    type Output = Vec<FavoriteAnime>;

    fn fetch(
        identifier: Identifier,
        url: String,
        parameters: Vec<(String, String)>,
    ) -> Result<Self::Response, Box<dyn std::error::Error>> {
        fetch_favorited_anime(identifier, url, parameters)
    }

    fn from_response(response: Self::Response) -> Self::Output {
        response.data.anime
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum DeleteOrUpdate {
    Updated(MyListStatus),           // For PUT - returns object
    Deleted(Vec<serde_json::Value>), // For DELETE - returns []
}

impl Update for Anime {
    type Response = DeleteOrUpdate;

    fn get_method(&self) -> &'static str {
        if !status_is_known(self.my_list_status.status.clone()) {
            "DELETE"
        } else {
            "PUT"
        }
    }

    fn get_parameters(&self) -> Vec<(String, String)> {
        vec![]
    }
    fn get_belonging_list(&self) -> String {
        "anime".to_string()
    }

    fn get_id(&self) -> usize {
        self.id
    }

    fn get_headers(&self, token: String) -> Vec<(String, String)> {
        if !status_is_known(self.my_list_status.status.clone()) {
            vec![("Authorization".to_string(), format!("Bearer {}", token))]
        } else {
            vec![
                ("Authorization".to_string(), format!("Bearer {}", token)),
                ("Content-Type".to_string(), "application/x-www-form-urlencoded".to_string()),
            ]
        }
    }

    fn get_body(&self) -> Option<String> {
        if !status_is_known(self.my_list_status.status.clone()) {
            return None;
        }

        Some(format!(
            "status={}&score={}&num_watched_episodes={}",
            watch_status_to_api(self.my_list_status.status.clone()),
            self.my_list_status.score,
            self.my_list_status.num_episodes_watched,
        ))
    }

    fn to_offline_response(&self) -> Self::Response {
        if !status_is_known(self.my_list_status.status.clone()) {
            DeleteOrUpdate::Deleted(vec![])
        } else {
            let mut normalized = self.my_list_status.clone();
            normalized.status = watch_status_from_api(normalized.status);
            DeleteOrUpdate::Updated(normalized)
        }
    }

    fn pre_update(&mut self) {
        self.my_list_status.updated_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(); 
    }
}

impl HasDisplayableImage for FavoriteAnime {
    fn get_displayable_image(&self) -> Option<(usize, String)> {
        Some((self.id, self.image.clone()))
    }
}

impl Storable for Anime {
    type Id = usize;

    fn get_id(&self) -> Self::Id {
        self.id
    }
}
