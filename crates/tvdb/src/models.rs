use serde;

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
pub struct Series {
    pub id: u32,
    #[serde(rename="seriesName")]
    pub name: String,
    #[serde(rename="firstAired")]
    pub first_aired: String,
    pub status: String,
    pub overview: Option<String>,
}

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
}

