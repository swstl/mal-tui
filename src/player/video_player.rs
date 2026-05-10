use crate::mal::models::anime::Anime;
use std::{io::ErrorKind, process::Command};
use regex::Regex;

#[derive(Debug, Clone)]
pub enum PlayError {
    NotReleased(Box<Anime>),
    CommandFailed {
        stderr: String,
        exit_code: i32,
        stdout: String,
    },
    NotFound(String),
    NoResults(String),
    Other(String),
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct PlayResult {
    pub episode: u32,
    pub current_time: String,
    pub total_time: String,
    pub percentage: u8,
    pub fully_watched: bool,
    pub is_completed: bool,
}


pub struct VideoPlayer {
    ansi_regex: Regex,

    //mpv regex:
    av_regex: Regex,
    exit_regex: Regex,
}

impl VideoPlayer {

    pub fn new() -> Self {
        VideoPlayer {
            ansi_regex: Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\([AB]|\r|\x1b[78]").unwrap(),
            av_regex: Regex::new(r"AV: (\d{2}:\d{2}:\d{2}) / (\d{2}:\d{2}:\d{2}) \((\d+)%\)")
                .unwrap(),
            exit_regex: Regex::new(r"Exiting\.\.\. \((.*?)\)").unwrap(),
        }
    }
    pub fn play(&self, info: &(String, Option<String>), episode: u32) -> Result<PlayResult, PlayError> {
        let play_info = self.launch_mpv(info)?;
        let result = self.extract_play_info(&play_info, episode).ok_or_else(|| {
            PlayError::Other("player did not return any play information".to_string())
        })?;
        Ok(result)
    }

    pub fn launch_mpv(&self, info: &(String, Option<String>)) -> Result<String, PlayError> {
        let mut cmd = Command::new("mpv");

        if let Some(referer) = &info.1 {
            cmd.arg(format!("--referrer={}", referer));
        }

        let output = cmd.arg(&info.0).output().map_err(|e| {
            if e.kind() == ErrorKind::NotFound {
                PlayError::NotFound("mpv is not installed or not found in PATH".to_string())
            } else {
                PlayError::Other(format!("Error running mpv: \n{}", e))
            }
        })?;

        let messy_stdout = String::from_utf8_lossy(&output.stdout);
        let messy_stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = self.ansi_regex.replace_all(&messy_stdout, "").to_string();
        let stderr = self.ansi_regex.replace_all(&messy_stderr, "").to_string();
        let exit_code = output.status.code().unwrap_or(-1);
        // mpv exit codes: 0 = clean EOF, 4 = quit by user. Both are success.
        let success = matches!(exit_code, 0 | 4);
        if !success {
            if stderr.contains("No results found!") {
                return Err(PlayError::NoResults(stderr));
            } else {
                return Err(PlayError::CommandFailed {
                    stderr,
                    exit_code,
                    stdout,
                });
            }
        }
        Ok(stdout)
    }

    pub fn extract_play_info(&self, stdout: &str, episode: u32) -> Option<PlayResult> {
        // return default if no output
        if stdout.is_empty() {
            return Some(PlayResult {
                current_time: "00:00:00".to_string(),
                total_time: "00:00:00".to_string(),
                is_completed: false,
                fully_watched: false,
                percentage: 0,
                episode,
            });
        }

        let last_av = if let Some(last_av) = stdout.rfind("AV: ") {
            let last_stdout = &stdout[last_av..];
            self.av_regex.captures(last_stdout)?
        } else {
            return None;
        };

        let exit_reason = self
            .exit_regex
            .captures(stdout)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str());

        let percentage = last_av[3].parse().unwrap_or(0);

        Some(PlayResult {
            current_time: last_av[1].to_string(),
            total_time: last_av[2].to_string(),
            is_completed: percentage >= 90,
            fully_watched: exit_reason == Some("End of file"),
            percentage,
            episode,
        })
    }
}
