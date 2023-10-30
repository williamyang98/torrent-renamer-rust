use serde;
use serde_with;

#[serde_with::skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Series {
    pub id: u32,
    #[serde(rename="seriesName")]
    pub name: String,
    #[serde(rename="firstAired")]
    pub first_aired: Option<String>,
    pub status: Option<String>,
    pub overview: Option<String>,
    pub genre: Option<Vec<String>>,
    pub aliases: Option<Vec<String>>,
    pub rating: Option<String>,
    pub slug: Option<String>,
    pub language: Option<String>,
    // external links
    #[serde(rename="imdbId")]
    pub imdb_id: Option<String>,
    #[serde(rename="zap2itId")]
    pub zap2_it_id: Option<String>,
    // links to images
    pub poster: Option<String>,
    pub banner: Option<String>,
    pub fanart: Option<String>,
    // network info
    pub network: Option<String>,
    #[serde(rename="networkId")]
    pub network_id: Option<String>,
    pub runtime: Option<String>,
    #[serde(rename="airsDayOfWeek")]
    pub airs_day_of_week: Option<String>,
    #[serde(rename="airsTime")]
    pub airs_time: Option<String>,
    // misc
    #[serde(rename="lastUpdated")]
    pub last_updated: Option<u32>,
}

#[serde_with::skip_serializing_none]
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Episode {
    pub id: u32,
    #[serde(rename="airedSeason")]
    pub season: u32,
    #[serde(rename="airedEpisodeNumber")]
    pub episode: u32,
    #[serde(rename="firstAired")]
    pub first_aired: Option<String>,
    #[serde(rename="episodeName")]
    pub name: Option<String>,
    pub overview: Option<String>,
    // production info
    pub writers: Option<Vec<String>>,
    pub directors: Option<Vec<String>>,
    #[serde(rename="guestStars")]
    pub guest_stars: Option<Vec<String>>,
    #[serde(rename="contentRating")]
    pub rating: Option<String>,
    // external links
    #[serde(rename="imdbId")]
    pub imdb_id: Option<String>,
    // links to images
    #[serde(rename="filename")]
    pub image_filename: Option<String>,
    // internal links
    #[serde(rename="seriesId")]
    pub series_id: Option<u32>,
    #[serde(rename="airedSeasonID")]
    pub season_id: Option<u32>,
}

