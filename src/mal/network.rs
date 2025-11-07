use super::models::anime::{AnimeResponse, FavoriteResponse};
use super::models::user::User;
use cached::proc_macro::cached;
use std::fmt::Debug;
use std::io::Read;
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use ureq::{Agent, Error};
use url::Url;

#[macro_export]
macro_rules! params {
    ($($key:expr => $value:expr),* $(,)?) => {
        vec![$(($key.to_string(), $value.to_string())),*]
    };
}

// this proxy url is just used to access a local cache server, for debugging and development
// pub const PROXY: &str = "http://localhost:1111/proxy?url=";
pub const PROXY: &str = "";
const MAX_RETRIES: u32 = 5;
static AGENT: OnceLock<Agent> = OnceLock::new();
fn get_agent() -> &'static Agent {
    AGENT.get_or_init(|| {
        Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .into()
    })
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Identifier {
    pub auth_token: Option<String>,
    pub client_id: Option<String>,
}

impl Identifier {
    pub fn new(auth_token: Option<String>, client_id: Option<String>) -> Self {
        Self {
            auth_token,
            client_id,
        }
    }

    pub fn to_headers(&self) -> Vec<(String, String)> {
        let mut headers = Vec::new();
        if let Some(token) = &self.auth_token {
            headers.push(("Authorization".to_string(), format!("Bearer {}", token)));
        }
        if let Some(client_id) = &self.client_id {
            headers.push(("X-MAL-Client-ID".to_string(), client_id.clone()));
        }
        headers
    }
}

#[cached(size = 2000, result = true)]
pub fn fetch_image(uri: String) -> Result<image::DynamicImage, String> {
    let url = Url::parse(&uri).map_err(|e| format!("Invalid URL: {}", e))?;

    let agent = get_agent();

    match url.scheme() {
        "http" | "https" => loop {
            match agent.get(&format!("{}{}", PROXY, uri)).call() {
                Ok(mut response) => {
                    let mut reader = response.body_mut().as_reader();
                    let mut buffer = Vec::new();
                    reader.read_to_end(&mut buffer).map_err(|e| e.to_string())?;

                    return image::load_from_memory(&buffer).map_err(|e| e.to_string());
                }
                Err(Error::StatusCode(code)) => return Err(format!("HTTP error: {}", code)),
                Err(e) => {
                    let error_message = e.to_string().to_lowercase();
                    let error_is_timeout =
                        error_message.contains("timeout") || error_message.contains("timed out");

                    if !error_is_timeout {
                        return Err(format!("Request failed: {}", e));
                    }
                }
            }
        },
        "file" => {
            let path = url
                .to_file_path()
                .map_err(|_| "Invalid file URL".to_string())?;
            image::open(path).map_err(|e| e.to_string())
        }
        _ => Err("Unsupported URL scheme".to_string()),
    }
}

#[cached(size = 2000, result = true)]
pub fn fetch_anime(
    identifier: Identifier,
    url: String,
    parameters: Vec<(String, String)>,
) -> Result<AnimeResponse, Box<dyn std::error::Error>> {
    send_request::<AnimeResponse>(
        "GET", //
        url,
        parameters,
        identifier.to_headers(),
        None,
    )
}

#[cached(result = true)]
pub fn fetch_user(
    identifier: Identifier,
    url: String,
    parameters: Vec<(String, String)>,
) -> Result<User, Box<dyn std::error::Error>> {
    send_request::<User>(
        "GET", //
        url,
        parameters,
        identifier.to_headers(),
        None,
    )
}

#[cached(result = true)]
pub fn fetch_favorited_anime(
    identifier: Identifier,
    url: String,
    parameters: Vec<(String, String)>,
) -> Result<FavoriteResponse, Box<dyn std::error::Error>> {
    send_request::<FavoriteResponse>(
        "GET", //
        url,
        parameters,
        identifier.to_headers(),
        None,
    )
}

fn build_url(
    base_url: &str,
    parameters: &[(String, String)],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut url = Url::parse(base_url)?;

    for (key, value) in parameters {
        url.query_pairs_mut().append_pair(key, value);
    }

    let target_url = url.to_string();
    Ok(format!("{}{}", PROXY, target_url))
}

// not cacheable since T
pub fn send_request<T>(
    method: &str,
    url: String,
    parameters: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    body: Option<&str>,
) -> Result<T, Box<dyn std::error::Error>>
where
    T: serde::de::DeserializeOwned + Debug,
{
    let final_url =
        build_url(&url, &parameters).map_err(|e| format!("Failed to build proxied URL: {}", e))?;

    let agent = get_agent();

    for attempt in 0..MAX_RETRIES {
        // create request
        let result = match method {
            "GET" => {
                let mut request = agent.get(&final_url);
                for (key, value) in &headers {
                    request = request.header(key, value);
                }
                request.call()
            }

            "PATCH" => {
                let mut request = agent.patch(&final_url);
                for (key, value) in &headers {
                    request = request.header(key, value);
                }
                request.send(body.unwrap_or(""))
            }

            "PUT" => {
                let mut request = agent.put(&final_url);
                // .header("Content-type", "application/x-www-form-urlencoded")
                for (key, value) in &headers {
                    request = request.header(key, value);
                }
                request.send(body.unwrap_or(""))
            }

            "POST" => {
                let mut request = agent.post(&final_url);
                for (key, value) in &headers {
                    request = request.header(key, value);
                }
                request.send(body.unwrap_or(""))
            }

            "DELETE" => {
                let mut request = agent.delete(&final_url);
                for (key, value) in &headers {
                    request = request.header(key, value);
                }
                request.call()
            }

            _ => return Err(format!("Unsupported HTTP method: {}", method).into()),
        };

        // check for errors
        match result {
            // all good
            Ok(mut response) => return Ok(response.body_mut().read_json::<T>()?),

            // request successful but with an error status code
            Err(ureq::Error::StatusCode(status)) => {
                return Err(format!("HTTP error: {}", status).into());
            }

            // request failed due to network error or timeout etc
            Err(e) => {
                let error_message = e.to_string().to_lowercase();
                let error_is_timeout =
                    error_message.contains("timeout") || error_message.contains("timed out");

                if !error_is_timeout {
                    return Err(format!("Request failed: {}", e).into());
                }

                if attempt >= MAX_RETRIES - 1 {
                    return Err(format!("max retries exceeded: {}, {}", MAX_RETRIES, e).into());
                }

                let delay = Duration::from_millis(2000 * (attempt + 1) as u64);
                thread::sleep(delay);
            }
        }
    }

    Err("All retry attempts failed".into())
}

pub fn send_request_expect_text(
    method: &str,
    url: String,
    parameters: Vec<(String, String)>,
    headers: Vec<(String, String)>,
    body: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let final_url =
        build_url(&url, &parameters).map_err(|e| format!("Failed to build proxied URL: {}", e))?;

    let agent = get_agent();

    for attempt in 0..MAX_RETRIES {
        let result = match method {
            "GET" => {
                let mut req = agent.get(&final_url);
                for (k, v) in &headers {
                    req = req.header(k, v);
                }
                req.call()
            }
            "PATCH" => {
                let mut req = agent.patch(&final_url);
                for (k, v) in &headers {
                    req = req.header(k, v);
                }
                req.send(body.unwrap_or(""))
            }
            "PUT" => {
                let mut req = agent.put(&final_url);
                for (k, v) in &headers {
                    req = req.header(k, v);
                }
                req.send(body.unwrap_or(""))
            }
            "POST" => {
                let mut req = agent.post(&final_url);
                for (k, v) in &headers {
                    req = req.header(k, v);
                }
                req.send(body.unwrap_or(""))
            }
            "DELETE" => {
                let mut req = agent.delete(&final_url);
                for (k, v) in &headers {
                    req = req.header(k, v);
                }
                req.call()
            }
            _ => return Err(format!("Unsupported HTTP method: {}", method).into()),
        };

        match result {
            Ok(mut resp) => return Ok(resp.body_mut().read_to_string()?),
            Err(ureq::Error::StatusCode(status)) => {
                return Err(format!("HTTP error: {}", status).into());
            }
            Err(e) => {
                let em = e.to_string().to_lowercase();
                let is_timeout = em.contains("timeout") || em.contains("timed out");
                if !is_timeout {
                    return Err(format!("Request failed: {}", e).into());
                }
                if attempt >= MAX_RETRIES - 1 {
                    return Err(format!("max retries exceeded: {}, {}", MAX_RETRIES, e).into());
                }
                thread::sleep(Duration::from_millis(2000 * (attempt + 1) as u64));
            }
        }
    }

    Err("All retry attempts failed".into())
}

pub trait Fetchable: Sized {
    type Response;
    type Output;

    fn fetch(
        token: Identifier,
        url: String,
        parameters: Vec<(String, String)>,
    ) -> Result<Self::Response, Box<dyn std::error::Error>>;

    fn from_response(response: Self::Response) -> Self::Output;
}

pub trait Update: Sized {
    type Response: serde::de::DeserializeOwned + Debug + Send;

    fn get_method(&self) -> &'static str;
    fn get_headers(&self, token: String) -> Vec<(String, String)>;
    fn get_parameters(&self) -> Vec<(String, String)>;
    fn get_body(&self) -> Option<String>;
    fn get_id(&self) -> usize;
    fn get_belonging_list(&self) -> String;

    fn update(
        &self,
        token: String,
        endpoint: String,
    ) -> Result<(usize, Self::Response), Box<dyn std::error::Error>> {
        let update = send_request::<Self::Response>(
            self.get_method(),
            endpoint,
            self.get_parameters(),
            self.get_headers(token),
            self.get_body().as_deref(),
        )?;
        Ok((self.get_id(), update))
    }
}
