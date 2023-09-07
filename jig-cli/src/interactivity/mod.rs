use crate::commands::shared::UseFilter;
use crate::config::Config;
use chrono::{Utc, Weekday};
use color_eyre::eyre::{eyre, Result, WrapErr};
use inquire::DateSelect;
use jira::{
    types::{Issue, IssueKey},
    JiraAPIClient,
};

#[cfg(feature = "cloud")]
mod filter;

// Might be useful one day
#[allow(dead_code)]
pub fn issue_key_from_branch_or_prompt(
    client: &JiraAPIClient,
    cfg: &Config,
    head_name: String,
) -> Result<IssueKey> {
    if let Ok(issue_key) = IssueKey::try_from(head_name) {
        return Ok(query_issue_details(client, issue_key)?.key);
    }

    let issues = query_issues_with_retry(client, cfg)?;

    #[cfg(not(feature = "fuzzy"))]
    let issue_key = prompt_user_with_issue_select(issues)?.key;
    #[cfg(feature = "fuzzy")]
    let issue_key = filter::prompt_user_with_issue_select(issues)?.key;

    Ok(issue_key)
}

pub fn issue_from_branch_or_prompt(
    client: &JiraAPIClient,
    cfg: &Config,
    head_name: String,
    _use_filter: UseFilter,
) -> Result<Issue> {
    if let Ok(issue_key) = IssueKey::try_from(head_name) {
        return query_issue_details(client, issue_key);
    }

    #[cfg(not(feature = "cloud"))]
    let query = cfg.issue_query.clone();
    #[cfg(feature = "cloud")]
    let query = match _use_filter.value {
        true => filter::pick_filter(cfg, client.search_filters(None)?.filters)?,
        false => cfg.issue_query.clone(),
    };

    let issues = override_query_issues_with_retry(client, &query, &cfg.retry_query)?;

    #[cfg(not(feature = "fuzzy"))]
    let issue = prompt_user_with_issue_select(issues);
    #[cfg(feature = "fuzzy")]
    let issue = filter::prompt_user_with_issue_select(issues);
    issue
}

#[cfg(not(feature = "fuzzy"))]
pub fn prompt_user_with_issue_select(issues: Vec<Issue>) -> Result<Issue> {
    use inquire::Select;

    if issues.is_empty() {
        Err(eyre!("Select Prompt: Empty issue list"))?
    }

    Select::new("Jira issue:", issues)
        .prompt()
        .wrap_err("No issue selected")
}

pub fn get_date(cfg: &Config, force_toggle_prompt: bool) -> Result<String> {
    // Jira sucks and can't parse correct rfc3339 due to the ':' in tz.. https://jira.atlassian.com/browse/JRASERVER-61378
    // Fix: https://docs.rs/chrono/latest/chrono/format/strftime/index.html
    let now = Utc::now().format("%FT%X%.3f%z").to_string();
    let mut do_prompt = cfg.always_confirm_date.unwrap_or(true);

    if force_toggle_prompt {
        do_prompt = !do_prompt;
    }

    if do_prompt {
        let date = DateSelect::new("")
            .with_week_start(Weekday::Mon)
            .prompt()?
            .to_string();

        // A lot prettier than a complex of format!()
        Ok(date + &now[10..])
    } else {
        Ok(now)
    }
}

pub fn query_issue_details(client: &JiraAPIClient, issue_key: IssueKey) -> Result<Issue> {
    let issues = client
        .query_issues(&format!("issuekey = {}", issue_key))?
        .issues
        .unwrap();

    match issues.first() {
        Some(i) => Ok(i.to_owned()),
        None => Err(eyre!("Error issue not found: {}", issue_key)),
    }
}

pub fn override_query_issues_with_retry(
    client: &JiraAPIClient,
    issue_query: &String,
    retry_query: &String,
) -> Result<Vec<Issue>> {
    let issues = match client
        .query_issues(issue_query)
        .wrap_err("First issue query failed")
    {
        Ok(issue_body) => issue_body.issues.unwrap(),
        Err(_) => client
            .query_issues(retry_query)
            .wrap_err(eyre!("Retry query failed"))?
            .issues
            .unwrap(),
    };

    Ok(issues)
}

pub fn query_issues_with_retry(client: &JiraAPIClient, cfg: &Config) -> Result<Vec<Issue>> {
    override_query_issues_with_retry(client, &cfg.issue_query, &cfg.retry_query)
}
