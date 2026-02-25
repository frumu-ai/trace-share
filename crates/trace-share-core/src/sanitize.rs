use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

use crate::models::CanonicalEvent;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SanitizationReport {
    pub total_redactions: usize,
    pub secret_redactions: usize,
    pub email_redactions: usize,
    pub ip_redactions: usize,
    pub path_redactions: usize,
    pub sample_redacted: Vec<String>,
}

pub fn sanitize_events(events: &[CanonicalEvent]) -> (Vec<CanonicalEvent>, SanitizationReport) {
    let mut report = SanitizationReport::default();
    let mut out = events.to_vec();
    apply_gitleaks_if_available(&mut out, &mut report);

    for event in &mut out {
        let before = event.text.clone();
        event.text = redact_text(&event.text, &mut report);
        if before != event.text && report.sample_redacted.len() < 5 {
            report.sample_redacted.push(event.text.clone());
        }
    }

    (out, report)
}

pub fn contains_sensitive_patterns(text: &str) -> bool {
    let mut probe = text.to_string();
    for marker in [
        "[REDACTED]",
        "[REDACTED_EMAIL]",
        "[REDACTED_IP]",
        "[REDACTED_PATH]",
        "[REDACTED_QUERY]",
        "[REDACTED_GITLEAKS]",
        "[REDACTED_JWT]",
        "[REDACTED_PEM]",
        "[REDACTED_USERHOST]",
        "[REDACTED_ENTROPY]",
    ] {
        probe = probe.replace(marker, "");
    }

    let token_re = token_regex();
    let bearer_re = bearer_regex();
    let jwt_re = jwt_regex();
    let pem_re = pem_private_key_regex();
    let email_re = email_regex();
    let ip_re = ip_regex();
    let url_query_re = url_query_regex();
    let user_host_re = user_host_regex();
    let host_assign_re = host_assignment_regex();
    let path_re = path_regex();

    token_re.is_match(&probe)
        || bearer_re.is_match(&probe)
        || jwt_re.is_match(&probe)
        || pem_re.is_match(&probe)
        || email_re.is_match(&probe)
        || ip_re.is_match(&probe)
        || user_host_re.is_match(&probe)
        || host_assign_re.is_match(&probe)
        || path_re.is_match(&probe)
        || url_query_re.is_match(&probe)
        || contains_high_entropy_token(&probe)
}

fn redact_text(input: &str, report: &mut SanitizationReport) -> String {
    let token_re = token_regex();
    let bearer_re = bearer_regex();
    let jwt_re = jwt_regex();
    let pem_re = pem_private_key_regex();
    let email_re = email_regex();
    let ip_re = ip_regex();
    let path_re = path_regex();
    let url_query_re = url_query_regex();
    let user_host_re = user_host_regex();
    let host_assign_re = host_assignment_regex();

    let mut text = input.to_string();

    let n = token_re.find_iter(&text).count();
    if n > 0 {
        text = token_re.replace_all(&text, "$1=[REDACTED]").to_string();
        report.secret_redactions += n;
        report.total_redactions += n;
    }

    let n = bearer_re.find_iter(&text).count();
    if n > 0 {
        text = bearer_re.replace_all(&text, "$1 [REDACTED]").to_string();
        report.secret_redactions += n;
        report.total_redactions += n;
    }

    let n = jwt_re.find_iter(&text).count();
    if n > 0 {
        text = jwt_re.replace_all(&text, "[REDACTED_JWT]").to_string();
        report.secret_redactions += n;
        report.total_redactions += n;
    }

    let n = pem_re.find_iter(&text).count();
    if n > 0 {
        text = pem_re.replace_all(&text, "[REDACTED_PEM]").to_string();
        report.secret_redactions += n;
        report.total_redactions += n;
    }

    let n = email_re.find_iter(&text).count();
    if n > 0 {
        text = email_re.replace_all(&text, "[REDACTED_EMAIL]").to_string();
        report.email_redactions += n;
        report.total_redactions += n;
    }

    let n = ip_re.find_iter(&text).count();
    if n > 0 {
        text = ip_re.replace_all(&text, "[REDACTED_IP]").to_string();
        report.ip_redactions += n;
        report.total_redactions += n;
    }

    let n = path_re.find_iter(&text).count();
    if n > 0 {
        text = path_re.replace_all(&text, "[REDACTED_PATH]").to_string();
        report.path_redactions += n;
        report.total_redactions += n;
    }

    let n = user_host_re.find_iter(&text).count();
    if n > 0 {
        text = user_host_re
            .replace_all(&text, "[REDACTED_USERHOST]")
            .to_string();
        report.secret_redactions += n;
        report.total_redactions += n;
    }

    let n = host_assign_re.find_iter(&text).count();
    if n > 0 {
        text = host_assign_re
            .replace_all(&text, "$1=[REDACTED_USERHOST]")
            .to_string();
        report.secret_redactions += n;
        report.total_redactions += n;
    }

    let n = url_query_re.find_iter(&text).count();
    if n > 0 {
        text = url_query_re
            .replace_all(&text, "$1?[REDACTED_QUERY]")
            .to_string();
        report.secret_redactions += n;
        report.total_redactions += n;
    }

    if contains_high_entropy_token(&text) {
        text = redact_high_entropy_tokens(&text, report);
    }

    text
}

fn token_regex() -> Regex {
    Regex::new(
        r#"(?i)(api[_-]?key|access[_-]?key|token|secret|authorization|password|passwd)\s*[:=]\s*[^\s,"']+"#,
    )
    .unwrap()
}

fn bearer_regex() -> Regex {
    Regex::new(r#"(?i)\b(authorization:?\s*bearer)\s+[A-Za-z0-9\-._~+/=]{8,}"#).unwrap()
}

fn jwt_regex() -> Regex {
    Regex::new(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b").unwrap()
}

fn pem_private_key_regex() -> Regex {
    Regex::new(r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----.*?-----END [A-Z0-9 ]*PRIVATE KEY-----")
        .unwrap()
}

fn email_regex() -> Regex {
    Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b").unwrap()
}

fn ip_regex() -> Regex {
    Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap()
}

fn path_regex() -> Regex {
    Regex::new(
        r"(?i)(?:/Users/[^/\s]+|/home/[^/\s]+|/root/[^/\s]*|[A-Za-z]:[\\/](?:[^\\/\s]+[\\/])*[^\\/\s]+)",
    )
    .unwrap()
}

fn url_query_regex() -> Regex {
    Regex::new(r"(https?://[^\s?]+)\?[^\s]+").unwrap()
}

fn user_host_regex() -> Regex {
    Regex::new(r"\b[A-Za-z0-9._-]{2,32}@[A-Za-z0-9._-]{2,64}\b").unwrap()
}

fn host_assignment_regex() -> Regex {
    Regex::new(r#"(?i)\b(hostname|host|user|username)\s*[:=]\s*([A-Za-z0-9._-]{2,64})"#).unwrap()
}

fn contains_high_entropy_token(text: &str) -> bool {
    text.split(|c: char| {
        c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | '(' | ')' | '[' | ']')
    })
    .any(is_high_entropy_token)
}

fn redact_high_entropy_tokens(text: &str, report: &mut SanitizationReport) -> String {
    let mut out = String::with_capacity(text.len());
    for token in text.split_inclusive(|c: char| c.is_whitespace()) {
        let trimmed = token.trim();
        if is_high_entropy_token(trimmed) {
            out.push_str(&token.replace(trimmed, "[REDACTED_ENTROPY]"));
            report.secret_redactions += 1;
            report.total_redactions += 1;
        } else {
            out.push_str(token);
        }
    }
    out
}

fn is_high_entropy_token(token: &str) -> bool {
    if token.len() < 24 {
        return false;
    }
    if token.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    if !token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_~+/=".contains(c))
    {
        return false;
    }
    let has_upper = token.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = token.chars().any(|c| c.is_ascii_lowercase());
    let has_digit = token.chars().any(|c| c.is_ascii_digit());
    (has_upper && has_lower && has_digit) || token.len() >= 32
}

fn apply_gitleaks_if_available(events: &mut [CanonicalEvent], report: &mut SanitizationReport) {
    let Some(gitleaks_bin) = find_gitleaks_binary() else {
        return;
    };

    let temp_dir =
        std::env::temp_dir().join(format!("trace-share-gitleaks-{}", uuid::Uuid::new_v4()));
    if fs::create_dir_all(&temp_dir).is_err() {
        return;
    }

    let mut file_map = Vec::new();
    for (i, event) in events.iter().enumerate() {
        let file_path = temp_dir.join(format!("event-{i}.txt"));
        if fs::write(&file_path, &event.text).is_ok() {
            file_map.push((i, file_path));
        }
    }

    if file_map.is_empty() {
        let _ = fs::remove_dir_all(&temp_dir);
        return;
    }

    let report_path = temp_dir.join("gitleaks-report.json");
    let output = Command::new(gitleaks_bin)
        .arg("detect")
        .arg("--no-git")
        .arg("--source")
        .arg(&temp_dir)
        .arg("--report-format")
        .arg("json")
        .arg("--report-path")
        .arg(&report_path)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output();

    let Ok(output) = output else {
        let _ = fs::remove_dir_all(&temp_dir);
        return;
    };

    // gitleaks exits with non-zero when leaks are found. We still parse report.
    if !report_path.exists() && !output.status.success() {
        let _ = fs::remove_dir_all(&temp_dir);
        return;
    }

    let report_text = fs::read_to_string(&report_path).unwrap_or_default();
    if report_text.trim().is_empty() {
        let _ = fs::remove_dir_all(&temp_dir);
        return;
    }

    let leaks = serde_json::from_str::<Vec<GitleaksFinding>>(&report_text).unwrap_or_default();
    for finding in leaks {
        if let Some(idx) = finding
            .file
            .as_deref()
            .and_then(extract_event_index)
            .filter(|idx| *idx < events.len())
        {
            if let Some(secret) = finding.secret.as_deref() {
                if !secret.is_empty() && events[idx].text.contains(secret) {
                    events[idx].text = events[idx].text.replace(secret, "[REDACTED_GITLEAKS]");
                    report.secret_redactions += 1;
                    report.total_redactions += 1;
                }
            }
        }
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

fn extract_event_index(path_text: &str) -> Option<usize> {
    let binding = PathBuf::from(path_text);
    let name = binding.file_name()?.to_str()?;
    let idx = name
        .strip_prefix("event-")?
        .strip_suffix(".txt")?
        .parse::<usize>()
        .ok()?;
    Some(idx)
}

fn find_gitleaks_binary() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).find_map(|dir| {
        let candidate = dir.join("gitleaks");
        if candidate.exists() {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate_exe = dir.join("gitleaks.exe");
            if candidate_exe.exists() {
                return Some(candidate_exe);
            }
        }
        None
    })
}

#[derive(Debug, Clone, Default, Deserialize)]
struct GitleaksFinding {
    #[serde(rename = "File")]
    file: Option<String>,
    #[serde(rename = "Secret")]
    secret: Option<String>,
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use crate::models::CanonicalEvent;

    use super::{contains_sensitive_patterns, sanitize_events};

    #[test]
    fn redacts_known_patterns() {
        let input = vec![CanonicalEvent {
            source: "x".to_string(),
            session_id: "s".to_string(),
            ts: Utc::now(),
            kind: "user_msg".to_string(),
            text: "token=abc123 email me at a@b.com from 127.0.0.1 /home/user/repo C:\\Users\\alice\\repo authorization: bearer ABCDEFGHIJ".to_string(),
            tool: None,
            meta: None,
        }];

        let (sanitized, report) = sanitize_events(&input);
        assert!(sanitized[0].text.contains("[REDACTED]"));
        assert!(sanitized[0].text.contains("[REDACTED_EMAIL]"));
        assert!(sanitized[0].text.contains("[REDACTED_IP]"));
        assert!(sanitized[0].text.contains("[REDACTED_PATH]"));
        assert!(
            sanitized[0]
                .text
                .to_ascii_lowercase()
                .contains("authorization=[redacted]")
        );
        assert!(report.total_redactions >= 4);
    }

    #[test]
    fn redacts_jwt_pem_and_entropy() {
        let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.cGF5bG9hZC12YWx1ZS0xMjM0NTY3ODkw.sigvalue1234567890ABCD";
        let pem = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0BAQEFAASC\n-----END PRIVATE KEY-----";
        let entropy = "AbCDef1234567890GhIjKlMnOpQrStUv";
        let input = vec![CanonicalEvent {
            source: "x".to_string(),
            session_id: "s".to_string(),
            ts: Utc::now(),
            kind: "user_msg".to_string(),
            text: format!("{jwt}\n{pem}\nsecret:{entropy}"),
            tool: None,
            meta: None,
        }];
        let (sanitized, _) = sanitize_events(&input);
        let out = &sanitized[0].text;
        assert!(out.contains("[REDACTED_JWT]"));
        assert!(out.contains("[REDACTED_PEM]"));
        assert!(out.contains("[REDACTED]") || out.contains("[REDACTED_ENTROPY]"));
    }

    #[test]
    fn extracts_gitleaks_event_index() {
        assert_eq!(super::extract_event_index("/tmp/x/event-12.txt"), Some(12));
        assert_eq!(super::extract_event_index("event-2.txt"), Some(2));
        assert_eq!(super::extract_event_index("random.txt"), None);
    }

    #[test]
    fn detects_sensitive_patterns() {
        assert!(contains_sensitive_patterns("token=abc123"));
        assert!(contains_sensitive_patterns("email is test@example.com"));
        assert!(contains_sensitive_patterns("visit https://x.y/z?a=1"));
        assert!(contains_sensitive_patterns(
            "cwd C:\\Users\\evang\\work\\trace-share"
        ));
        assert!(contains_sensitive_patterns(
            "eyJhbGciOiJIUzI1NiJ9.abc1234567.zyx0987654"
        ));
        assert!(contains_sensitive_patterns(
            "-----BEGIN PRIVATE KEY-----abc-----END PRIVATE KEY-----"
        ));
        assert!(!contains_sensitive_patterns("clean text only"));
        assert!(!contains_sensitive_patterns("token=[REDACTED]"));
    }
}
