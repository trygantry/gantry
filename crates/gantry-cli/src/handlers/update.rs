use crate::output::Output;

pub trait ReleaseFetcher {
    fn fetch_latest_release(&self) -> Result<(String, String), String>;
}

pub struct HttpReleaseFetcher;

impl ReleaseFetcher for HttpReleaseFetcher {
    fn fetch_latest_release(&self) -> Result<(String, String), String> {
        let url = "https://api.github.com/repos/trygantry/gantry/releases/latest";
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build();
        let agent: ureq::Agent = config.into();

        let user_agent = format!("gantry-cli/{}", env!("CARGO_PKG_VERSION"));
        let resp_res = agent
            .get(url)
            .header("User-Agent", &user_agent)
            .header("Accept", "application/vnd.github.v3+json")
            .call();

        let mut resp = match resp_res {
            Ok(r) => r,
            Err(ureq::Error::StatusCode(code)) => {
                if code == 403 || code == 429 {
                    return Err("GitHub rate limit exceeded. Please try again later.".to_string());
                }
                return Err(format!("GitHub API returned error status: {}", code));
            }
            Err(e) => {
                return Err(format!("Network error: {}", e));
            }
        };

        if resp.status() == 403 || resp.status() == 429 {
            return Err("GitHub rate limit exceeded. Please try again later.".to_string());
        }

        if !resp.status().is_success() {
            return Err(format!(
                "GitHub API returned error status: {}",
                resp.status()
            ));
        }

        let body_str = resp
            .body_mut()
            .read_to_string()
            .map_err(|e| format!("Failed to read GitHub release body: {}", e))?;

        let json_val: serde_json::Value = serde_json::from_str(&body_str)
            .map_err(|e| format!("Failed to parse GitHub release JSON: {}", e))?;

        let tag_name = json_val
            .get("tag_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "GitHub response did not contain a valid tag_name".to_string())?
            .to_string();

        let html_url = json_val
            .get("html_url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "GitHub response did not contain a valid html_url".to_string())?
            .to_string();

        Ok((tag_name, html_url))
    }
}

#[derive(serde::Serialize)]
struct UpdateInfo {
    current_version: String,
    latest_version: String,
    up_to_date: bool,
    release_url: String,
}

pub fn handle(
    output: &Output,
    json_mode: bool,
    fetcher: &dyn ReleaseFetcher,
) -> Result<i32, String> {
    let current_tag = env!("CARGO_PKG_VERSION");
    let current_ver = semver::Version::parse(current_tag)
        .map_err(|e| format!("Failed to parse current version: {}", e))?;

    let (latest_tag, html_url) = fetcher.fetch_latest_release()?;

    let clean_tag = if let Some(stripped) = latest_tag.strip_prefix('v') {
        stripped
    } else {
        &latest_tag
    };

    let latest_ver = semver::Version::parse(clean_tag)
        .map_err(|e| format!("Failed to parse latest version '{}': {}", clean_tag, e))?;

    let up_to_date = current_ver >= latest_ver;

    if json_mode || output.json {
        let info = UpdateInfo {
            current_version: current_tag.to_string(),
            latest_version: clean_tag.to_string(),
            up_to_date,
            release_url: html_url,
        };
        output.print_json(&info);
    } else if up_to_date {
        output.print_stdout(&format!(
            "Gantry is up to date (current: {}, latest: {}).",
            current_ver, latest_ver
        ));
    } else {
        output.print_stdout(&format!(
            "A new version of Gantry is available: {} -> {} (Release notes: {})",
            current_ver, latest_ver, html_url
        ));
    }

    Ok(0)
}

#[cfg(test)]
pub struct MockReleaseFetcher {
    pub tag: String,
    pub url: String,
    pub should_error: bool,
}

#[cfg(test)]
impl ReleaseFetcher for MockReleaseFetcher {
    fn fetch_latest_release(&self) -> Result<(String, String), String> {
        if self.should_error {
            Err("Mock fetch error".to_string())
        } else {
            Ok((self.tag.clone(), self.url.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_checker() {
        let output = Output::new(true, false, true); // JSON, quiet mode

        // Case 1: Up to date
        let mock_fetcher_up_to_date = MockReleaseFetcher {
            tag: format!("v{}", env!("CARGO_PKG_VERSION")),
            url: "https://mock.url".to_string(),
            should_error: false,
        };
        let res = handle(&output, true, &mock_fetcher_up_to_date);
        assert_eq!(res, Ok(0));

        // Case 2: Out of date
        let mock_fetcher_out_of_date = MockReleaseFetcher {
            tag: "v999.0.0".to_string(),
            url: "https://mock.url".to_string(),
            should_error: false,
        };
        let res = handle(&output, true, &mock_fetcher_out_of_date);
        assert_eq!(res, Ok(0));

        // Case 3: Error
        let mock_fetcher_err = MockReleaseFetcher {
            tag: "".to_string(),
            url: "".to_string(),
            should_error: true,
        };
        let res = handle(&output, true, &mock_fetcher_err);
        assert!(res.is_err());
    }
}
