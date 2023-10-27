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

    pub aliases: Option<Vec<String>>,
    pub poster: Option<String>,
    pub banner: Option<String>,
    pub fanart: Option<String>,
    pub network: Option<String>,
    pub genre: Option<Vec<String>>,
    #[serde(rename="lastUpdated")]
    pub last_updated: Option<u32>,
    pub rating: Option<String>,
    pub slug: Option<String>,
    pub language: Option<String>,
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

    pub writers: Option<Vec<String>>,
    pub directors: Option<Vec<String>>,
    #[serde(rename="contentRating")]
    pub rating: Option<String>,
}

