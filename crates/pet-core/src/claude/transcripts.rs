//! Per-session / per-agent consumption from Claude Code's local transcripts.
//!
//! Layout (not documented, so handled tolerantly):
//! - `<projects>/<project-folder>/<session-id>.jsonl` main conversation;
//! - `<projects>/<project-folder>/<session-id>/subagents/*.jsonl` one file per
//!   subagent;
//! - and/or lines with `"isSidechain": true` inside the main file.
//!
//! Only token counters, model names and timestamps are read; message text is
//! never kept. Files are streamed line by line (they can be tens of MB).

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use super::pricing::estimate_cost_usd;
use crate::{AgentUsage, Error, Result, SessionUsage, TokenTotals};

const MAIN_AGENT: &str = "main";

#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "requestId")]
    request_id: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "isSidechain")]
    is_sidechain: Option<bool>,
    #[serde(rename = "agentId")]
    agent_id: Option<String>,
    message: Option<Message>,
}

#[derive(Deserialize)]
struct Message {
    id: Option<String>,
    model: Option<String>,
    usage: Option<Usage>,
}

#[derive(Deserialize)]
struct Usage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    cache_read_input_tokens: Option<u64>,
}

#[derive(Clone)]
struct Request {
    model: Option<String>,
    tokens: TokenTotals,
}

#[derive(Default)]
struct AgentAcc {
    /// Display name (first one seen).
    name: String,
    /// Deduplicated requests, in first-seen order; a repeated requestId
    /// replaces the earlier record (the last one wins).
    order: Vec<String>,
    requests: HashMap<String, Request>,
}

/// Most recently modified top-level session file under `projects_dir`
/// (`<projects>/<project>/<session>.jsonl`). Subagent files and legacy
/// top-level `agent-*.jsonl` sidechain files are excluded.
pub fn find_latest_session(projects_dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(SystemTime, PathBuf)> = None;
    for project in std::fs::read_dir(projects_dir).ok()?.flatten() {
        let Ok(files) = std::fs::read_dir(project.path()) else {
            continue;
        };
        for entry in files.flatten() {
            let path = entry.path();
            let is_jsonl = path.extension().is_some_and(|e| e == "jsonl");
            let is_agent = path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("agent-"));
            if !is_jsonl || is_agent {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            let Ok(modified) = meta.modified() else {
                continue;
            };
            if best.as_ref().is_none_or(|(t, _)| modified > *t) {
                best = Some((modified, path));
            }
        }
    }
    best.map(|(_, p)| p)
}

/// Summarize one session: totals plus one entry per agent ("main" first, then
/// subagents sorted by name).
pub fn summarize(session_file: &Path) -> Result<SessionUsage> {
    let session_id = session_file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| Error::Format("session path has no file name".into()))?;
    let project = session_file
        .parent()
        .and_then(Path::file_name)
        .map(|n| n.to_string_lossy().into_owned());

    let mut agents: HashMap<String, AgentAcc> = HashMap::new();
    let mut last_activity: Option<DateTime<Utc>> = None;

    agents.entry(MAIN_AGENT.into()).or_default().name = MAIN_AGENT.into();
    read_file(session_file, None, &mut agents, &mut last_activity)?;

    let subagent_dir = session_file.with_extension("").join("subagents");
    if let Ok(read_dir) = std::fs::read_dir(&subagent_dir) {
        let mut files: Vec<PathBuf> = read_dir
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
            .collect();
        files.sort();
        for path in files {
            let Some(stem) = path.file_stem().map(|s| s.to_string_lossy().into_owned()) else {
                continue;
            };
            // A broken subagent file must not hide the rest of the session.
            let _ = read_file(&path, Some(&stem), &mut agents, &mut last_activity);
        }
    }

    let mut list: Vec<AgentUsage> = agents.into_values().map(finish_agent).collect();
    list.sort_by(|a, b| (a.agent != MAIN_AGENT, &a.agent).cmp(&(b.agent != MAIN_AGENT, &b.agent)));

    let mut tokens = TokenTotals::default();
    let mut cost: Option<f64> = None;
    for a in &list {
        tokens.add(&a.tokens);
        if let Some(c) = a.cost_usd_est {
            *cost.get_or_insert(0.0) += c;
        }
    }

    Ok(SessionUsage {
        session_id,
        project,
        last_activity,
        tokens,
        cost_usd_est: cost,
        agents: list,
    })
}

/// Normalize "agent-abc" / "abc" to one key so a sidechain line and its
/// subagent file land in the same bucket.
fn agent_key(name: &str) -> String {
    name.strip_prefix("agent-").unwrap_or(name).to_string()
}

fn read_file(
    path: &Path,
    forced_agent: Option<&str>,
    agents: &mut HashMap<String, AgentAcc>,
    last_activity: &mut Option<DateTime<Utc>>,
) -> Result<()> {
    let reader = BufReader::new(File::open(path)?);
    for (line_no, raw) in reader.split(b'\n').enumerate() {
        let raw = raw?;
        // Cheap pre-filter: skip big non-assistant lines without parsing.
        if !contains(&raw, b"\"assistant\"") {
            continue;
        }
        let Ok(line) = serde_json::from_slice::<Line>(&raw) else {
            continue;
        };
        if line.kind.as_deref() != Some("assistant") {
            continue;
        }
        let Some(msg) = line.message else { continue };
        let Some(usage) = msg.usage else { continue };
        if msg.model.as_deref() == Some("<synthetic>") {
            continue;
        }

        if let Some(ts) = line
            .timestamp
            .as_deref()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Utc))
        {
            if last_activity.is_none_or(|l| ts > l) {
                *last_activity = Some(ts);
            }
        }

        let agent_name: String = match forced_agent {
            Some(n) => n.to_string(),
            None if line.is_sidechain == Some(true) => line
                .agent_id
                .filter(|a| !a.is_empty())
                .unwrap_or_else(|| "sidechain".into()),
            None => MAIN_AGENT.into(),
        };
        let acc = agents.entry(agent_key(&agent_name)).or_default();
        if acc.name.is_empty() {
            acc.name = agent_name;
        }

        // Same request repeated over several lines: keep one, the last.
        let key = line
            .request_id
            .or(msg.id)
            .unwrap_or_else(|| format!("line-{line_no}"));
        let req = Request {
            model: msg.model,
            tokens: TokenTotals {
                input: usage.input_tokens.unwrap_or(0),
                output: usage.output_tokens.unwrap_or(0),
                cache_creation: usage.cache_creation_input_tokens.unwrap_or(0),
                cache_read: usage.cache_read_input_tokens.unwrap_or(0),
            },
        };
        if acc.requests.insert(key.clone(), req).is_none() {
            acc.order.push(key);
        }
    }
    Ok(())
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn finish_agent(acc: AgentAcc) -> AgentUsage {
    let mut tokens = TokenTotals::default();
    let mut cost: Option<f64> = None;
    let mut model: Option<String> = None;
    for key in &acc.order {
        let r = &acc.requests[key];
        tokens.add(&r.tokens);
        if let Some(m) = &r.model {
            model = Some(m.clone());
            if let Some(c) = estimate_cost_usd(m, &r.tokens) {
                *cost.get_or_insert(0.0) += c;
            }
        }
    }
    AgentUsage {
        agent: acc.name,
        model,
        tokens,
        cost_usd_est: cost,
        requests: acc.order.len() as u32,
    }
}
