use url;
use reqwest;
use serde;
use serde_json;
use futures;
use std::sync::Arc;
use thiserror;

use crate::models::{Series, Episode};

const BASE_URL: &str = "https://api.thetvdb.com";

#[derive(serde::Deserialize)]
struct ResponseBody<'a> {
    #[serde(borrow)]
    data: &'a serde_json::value::RawValue,
}

#[derive(serde::Deserialize)]
struct ErrorBody {
    #[serde(rename="Error")]
    error: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("request failure: {}", .0)]
    RequestFailure(reqwest::Error),
    #[error("unexpected response: code={} body={}", .0, .1)]
    UnexpectedResponse(reqwest::StatusCode, String),
    #[error("json encode error: {}", .0)]
    JsonEncode(serde_json::Error),
    #[error("json decode error: {}", .0)]
    JsonDecode(serde_json::Error),
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct LoginInfo {
    pub apikey: String,
    pub userkey: String,
    pub username: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
pub struct LoginToken {
    pub token: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct EpisodesPageLinks {
    next: Option<u32>,
    last: Option<u32>,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct EpisodesPage {
    #[serde(rename="data")]
    episodes: Option<Vec<Episode>>,
    links: Option<EpisodesPageLinks>,    
}

pub struct LoginSession {
    client: Arc<reqwest::Client>,
    token: LoginToken,
}

pub async fn login(client: &reqwest::Client, login_info: &LoginInfo) -> Result<LoginToken, ApiError> {
    let res = client
        .post(format!("{}/login", BASE_URL))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(login_info).map_err(ApiError::JsonEncode)?)
        .send()
        .await
        .map_err(ApiError::RequestFailure)?;

    let status = res.status();
    let body = res.text().await.map_err(ApiError::RequestFailure)?;
    if !status.is_success() {
        let message: Result<ErrorBody, serde_json::Error> = serde_json::from_str(body.as_str());
        let error = match message {
            Ok(value) => value.error.as_str().to_string(),
            Err(_) => body,
        };
        return Err(ApiError::UnexpectedResponse(status, error));
    };

    let session: LoginToken = serde_json::from_str(body.as_str()).map_err(ApiError::JsonDecode)?; 
    Ok(session)
}

impl LoginSession {
    pub fn new<'b>(client: Arc<reqwest::Client>, token: &LoginToken) -> Self {
        Self {
            client,
            token: token.clone(),
        }
    }
}

impl LoginSession {
    pub async fn refresh_token(&mut self) -> Result<(), ApiError> {
        let token = self.get_new_token().await?;
        self.token = token;
        Ok(())
    }

    pub async fn get_new_token(&self) -> Result<LoginToken, ApiError> {
        let res = self.client
            .get(format!("{}/refresh_token", BASE_URL))
            .header("Authorization", format!("Bearer {}", self.token.token))
            .send()
            .await
            .map_err(ApiError::RequestFailure)?;
        
        let status = res.status();
        let body = res.text().await.map_err(ApiError::RequestFailure)?;
        if !status.is_success() {
            let message: Result<ErrorBody, serde_json::Error> = serde_json::from_str(body.as_str());
            let error = match message {
                Ok(value) => value.error.as_str().to_string(),
                Err(_) => body,
            };
            return Err(ApiError::UnexpectedResponse(status, error));
        };

        let token: LoginToken = serde_json::from_str(body.as_str()).map_err(ApiError::JsonDecode)?; 
        Ok(token)
    }

    pub async fn search_series(&self, name: &String) -> Result<Vec<Series>, ApiError> {
        let params = [("name", name)];
        let base_url = format!("{}/search/series", BASE_URL);
        let full_url = url::Url::parse_with_params(base_url.as_str(), &params).expect("Url is valid");
        let res = self.client
            .get(full_url.as_str())
            .header("Authorization", format!("Bearer {}", self.token.token))
            .send()
            .await
            .map_err(ApiError::RequestFailure)?;

        let status = res.status();
        let body = res.text().await.map_err(ApiError::RequestFailure)?;
        if !status.is_success() {
            let message: Result<ErrorBody, serde_json::Error> = serde_json::from_str(body.as_str());
            let error = match message {
                Ok(value) => value.error.as_str().to_string(),
                Err(_) => body,
            };
            return Err(ApiError::UnexpectedResponse(status, error));
        };

        let response_body: ResponseBody = serde_json::from_str(body.as_str()).map_err(ApiError::JsonDecode)?;
        let data: Vec<Series> = serde_json::from_str(response_body.data.get()).map_err(ApiError::JsonDecode)?;
        Ok(data)
    }

    pub async fn get_series(&self, id: u32) -> Result<Series, ApiError> {
        let res = self.client
            .get(format!("{}/series/{}", BASE_URL, id))
            .header("Authorization", format!("Bearer {}", self.token.token))
            .send()
            .await
            .map_err(ApiError::RequestFailure)?;

        let status = res.status();
        let body = res.text().await.map_err(ApiError::RequestFailure)?;
        if !status.is_success() {
            let message: Result<ErrorBody, serde_json::Error> = serde_json::from_str(body.as_str());
            let error = match message {
                Ok(value) => value.error.as_str().to_string(),
                Err(_) => body,
            };
            return Err(ApiError::UnexpectedResponse(status, error));
        };

        let response_body: ResponseBody = serde_json::from_str(body.as_str()).map_err(ApiError::JsonDecode)?;
        let series: Series = serde_json::from_str(response_body.data.get()).map_err(ApiError::JsonDecode)?;
        Ok(series)
    }

    async fn get_episodes_page(&self, id: u32, page: u32) -> Result<EpisodesPage, ApiError> {
        let res = self.client
            .get(format!("{}/series/{}/episodes?page={}", BASE_URL, id, page))
            .header("Authorization", format!("Bearer {}", self.token.token))
            .send()
            .await
            .map_err(ApiError::RequestFailure)?;
        
        let status = res.status();
        let body = res.text().await.map_err(ApiError::RequestFailure)?;
        if !status.is_success() {
            let message: Result<ErrorBody, serde_json::Error> = serde_json::from_str(body.as_str());
            let error = match message {
                Ok(value) => value.error.as_str().to_string(),
                Err(_) => body,
            };
            return Err(ApiError::UnexpectedResponse(status, error));
        };
        let page: EpisodesPage = serde_json::from_str(body.as_str()).map_err(ApiError::JsonDecode)?;
        Ok(page)
    }

    pub async fn get_episodes(&self, id: u32) -> Result<Vec<Episode>, ApiError> {
        let page_1 = match self.get_episodes_page(id, 1).await {
            Ok(page) => page,
            Err(err) => return Err(err),
        };

        let mut all_episodes: Vec<Episode> = Vec::new();
        if let Some(episodes) = page_1.episodes {
            all_episodes.extend_from_slice(episodes.as_slice());
        }

        if let Some(links) = page_1.links {
            let next_page = links.next.unwrap_or(0);
            let last_page = links.last.unwrap_or(0);
            let tasks: Vec<_> = (next_page..last_page)
                .map(|page| self.get_episodes_page(id, page))
                .collect();

            for page in futures::future::join_all(tasks).await.into_iter().flatten() {
                if let Some(episodes) = page.episodes {
                    all_episodes.extend_from_slice(episodes.as_slice());
                }
            }
        }

        Ok(all_episodes)
    }
}
