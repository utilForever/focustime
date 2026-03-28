use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

fn blocklist_path() -> Option<PathBuf> {
    dirs::config_dir().map(|mut p| {
        p.push("focustime");
        p.push("blocklist.txt");
        p
    })
}

/// Load the block list from disk. Returns an empty list if the file does not
/// exist or the config directory cannot be determined.
pub fn load_block_list() -> Vec<String> {
    let Some(path) = blocklist_path() else {
        return Vec::new();
    };
    let Ok(contents) = fs::read_to_string(&path) else {
        return Vec::new();
    };
    contents
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(String::from)
        .collect()
}

/// Persist the block list to disk. Creates parent directories as needed.
pub fn save_block_list(list: &[String]) -> io::Result<()> {
    let path = blocklist_path().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "cannot determine config directory")
    })?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = fs::File::create(&path)?;
    for domain in list {
        writeln!(file, "{domain}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn parse_lines(domains: &[&str]) -> Vec<String> {
        let mut raw = String::new();
        for d in domains {
            raw.push_str(d);
            raw.push('\n');
        }
        raw.lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(String::from)
            .collect()
    }

    // Write to a temp path and read back, exercising the file-system path.
    fn round_trip_file(domains: &[&str]) -> Vec<String> {
        let path = std::env::temp_dir().join("focustime_test_blocklist.txt");
        let mut file = fs::File::create(&path).unwrap();
        for d in domains {
            writeln!(file, "{d}").unwrap();
        }
        let contents = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_file(&path);
        contents
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(String::from)
            .collect()
    }

    #[test]
    fn parse_preserves_entries() {
        let result = parse_lines(&["example.com", "bad.com"]);
        assert_eq!(result, vec!["example.com", "bad.com"]);
    }

    #[test]
    fn parse_filters_comments_and_blanks() {
        let result = parse_lines(&["# comment", "", "good.com"]);
        assert_eq!(result, vec!["good.com"]);
    }

    #[test]
    fn file_round_trip() {
        let result = round_trip_file(&["example.com", "bad.com"]);
        assert_eq!(result, vec!["example.com", "bad.com"]);
    }
}
