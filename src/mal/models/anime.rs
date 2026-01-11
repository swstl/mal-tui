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
    pub watching: u64,
    pub completed: u64,
    pub on_hold: u64,
    pub dropped: u64,
    pub plan_to_watch: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Statistics {
    pub status: Status,
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
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FavoriteResponse {
    pub data: JikanData,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JikanData {
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

