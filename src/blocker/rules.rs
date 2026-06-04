#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteValidationError {
    EmptyHostname,
    MissingHostname,
    ContainsWhitespace,
    InvalidCharacter,
    InvalidLabel,
    MultipleHostnames,
}

impl SiteValidationError {
    pub fn message(self) -> &'static str {
        match self {
            SiteValidationError::EmptyHostname => "empty hostname",
            SiteValidationError::MissingHostname => "missing hostname",
            SiteValidationError::ContainsWhitespace => "contains whitespace",
            SiteValidationError::InvalidCharacter => "contains invalid characters",
            SiteValidationError::InvalidLabel => "invalid hostname format",
            SiteValidationError::MultipleHostnames => "multiple hostnames not allowed",
        }
    }
}

pub fn normalize_domain_rule(input: &str) -> Result<String, SiteValidationError> {
    normalize_domain_like_input(input, true)
}

pub fn normalize_domain_host(input: &str) -> Result<String, SiteValidationError> {
    normalize_domain_like_input(input, false)
}

pub fn domain_rule_matches_host(rule: &str, host: &str) -> bool {
    let Ok(rule) = normalize_domain_rule(rule) else {
        return false;
    };
    let Ok(host) = normalize_domain_host(host) else {
        return false;
    };
    if let Some(suffix) = rule.strip_prefix("*.") {
        return host.ends_with(&format!(".{suffix}"));
    }
    host == rule
}

fn normalize_domain_like_input(
    input: &str,
    allow_wildcard_prefix: bool,
) -> Result<String, SiteValidationError> {
    let hostname = extract_hostname_candidate(input)?;
    let (hostname, wildcard_prefix) = extract_wildcard_prefix(hostname, allow_wildcard_prefix)?;
    let hostname = strip_trailing_root_dots(strip_numeric_port(hostname)?);
    if hostname.is_empty() {
        return Err(SiteValidationError::MissingHostname);
    }
    validate_domain_host(&hostname)?;
    if wildcard_prefix && !hostname.contains('.') {
        return Err(SiteValidationError::InvalidLabel);
    }
    if wildcard_prefix {
        Ok(format!("*.{hostname}"))
    } else {
        Ok(hostname)
    }
}

fn extract_hostname_candidate(input: &str) -> Result<String, SiteValidationError> {
    let mut hostname = input.trim().to_lowercase();
    if hostname.is_empty() {
        return Err(SiteValidationError::EmptyHostname);
    }
    if let Some(sep) = hostname.find("://") {
        hostname = hostname[sep + 3..].to_string();
    }
    if let Some(pos) = hostname.find(['/', '?', '#']) {
        hostname.truncate(pos);
    }
    if let Some(at_pos) = hostname.rfind('@') {
        hostname = hostname[at_pos + 1..].to_string();
    }
    Ok(hostname)
}

fn extract_wildcard_prefix(
    hostname: String,
    allow_wildcard_prefix: bool,
) -> Result<(String, bool), SiteValidationError> {
    if let Some(stripped) = hostname.strip_prefix("*.") {
        if !allow_wildcard_prefix {
            return Err(SiteValidationError::InvalidCharacter);
        }
        return Ok((stripped.to_string(), true));
    }
    if let Some(stripped) = hostname.strip_prefix('.') {
        return Ok((stripped.to_string(), allow_wildcard_prefix));
    }
    Ok((hostname, false))
}

fn strip_numeric_port(mut hostname: String) -> Result<String, SiteValidationError> {
    if let Some(colon_pos) = hostname.rfind(':') {
        let port = &hostname[colon_pos + 1..];
        if hostname[..colon_pos].contains(':') || !port.chars().all(|c| c.is_ascii_digit()) {
            return Err(SiteValidationError::InvalidLabel);
        }
        hostname.truncate(colon_pos);
    }
    Ok(hostname)
}

fn strip_trailing_root_dots(mut hostname: String) -> String {
    while hostname.ends_with('.') {
        hostname.pop();
    }
    hostname
}

fn validate_domain_host(hostname: &str) -> Result<(), SiteValidationError> {
    if hostname.chars().any(char::is_whitespace) {
        return Err(SiteValidationError::ContainsWhitespace);
    }
    if !hostname
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
    {
        return Err(SiteValidationError::InvalidCharacter);
    }
    if hostname.starts_with('.')
        || hostname.ends_with('.')
        || hostname.contains("..")
        || hostname.len() > 253
    {
        return Err(SiteValidationError::InvalidLabel);
    }
    for label in hostname.split('.') {
        if label.is_empty() || label.len() > 63 || label.starts_with('-') || label.ends_with('-') {
            return Err(SiteValidationError::InvalidLabel);
        }
    }
    Ok(())
}
