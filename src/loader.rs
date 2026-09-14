//! Loads per-frame token usage from the Claude Science metadata database.

use std::path::Path;

use sqlite::{Connection, State};

/// One conversation frame with aggregate token usage.
#[derive(Debug)]
pub struct Frame {
    pub session_id: String,
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_usd: Option<f64>,
    pub timestamp_ms: i64,
}

/// Returns whether the path looks like a Claude Science metadata database
/// by preparing the exact projection the loader runs.
pub fn is_claude_science_database(path: &Path) -> bool {
    let connection = match sqlite::Connection::open_with_flags(
        path,
        sqlite::OpenFlags::new().with_read_only().with_no_mutex(),
    ) {
        Ok(connection) => connection,
        Err(_) => return false,
    };
    let mut statement = match connection
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'frames'")
    {
        Ok(statement) => statement,
        Err(_) => return false,
    };
    let has_frames = statement.next().ok() == Some(State::Row);
    drop(statement);
    if !has_frames {
        return false;
    }
    let usable = connection
        .prepare(
            "SELECT id, COALESCE(root_frame_id, id), model, input_tokens, output_tokens, \
             cache_read_tokens, cache_write_tokens, total_cost, updated_at FROM frames LIMIT 1",
        )
        .is_ok();
    usable
}

/// Reads every frame with token usage from one database.
pub fn read_frames(path: &Path) -> Result<Vec<Frame>, String> {
    let connection = sqlite::Connection::open_with_flags(
        path,
        sqlite::OpenFlags::new().with_read_only().with_no_mutex(),
    )
    .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    let query = if projects_table_is_usable(&connection) {
        "SELECT frames.id, COALESCE(frames.root_frame_id, frames.id), frames.model, \
         frames.input_tokens, frames.output_tokens, frames.cache_read_tokens, \
         frames.cache_write_tokens, frames.total_cost, frames.updated_at, projects.name \
         FROM frames LEFT JOIN projects ON projects.id = frames.project_id \
         WHERE frames.input_tokens IS NOT NULL AND frames.output_tokens IS NOT NULL"
    } else {
        "SELECT frames.id, COALESCE(frames.root_frame_id, frames.id), frames.model, \
         frames.input_tokens, frames.output_tokens, frames.cache_read_tokens, \
         frames.cache_write_tokens, frames.total_cost, frames.updated_at, NULL \
         FROM frames \
         WHERE frames.input_tokens IS NOT NULL AND frames.output_tokens IS NOT NULL"
    };
    let mut statement = connection
        .prepare(query)
        .map_err(|error| format!("{path:?}: {error}"))?;
    let mut frames = Vec::new();
    while let State::Row = statement
        .next()
        .map_err(|error| format!("{path:?}: {error}"))?
    {
        let timestamp_ms = statement.read::<Option<i64>, _>(8).ok().flatten();
        let timestamp_ms = match timestamp_ms {
            Some(timestamp_ms) if timestamp_ms > 0 => timestamp_ms,
            _ => continue,
        };
        frames.push(Frame {
            session_id: statement
                .read::<String, _>(1)
                .map_err(|error| format!("{path:?}: {error}"))?,
            model: normalize_model(
                &statement
                    .read::<String, _>(2)
                    .map_err(|error| format!("{path:?}: {error}"))?,
            ),
            input_tokens: non_negative(statement.read::<Option<i64>, _>(3).ok().flatten()),
            output_tokens: non_negative(statement.read::<Option<i64>, _>(4).ok().flatten()),
            cache_read_tokens: non_negative(statement.read::<Option<i64>, _>(5).ok().flatten()),
            cache_write_tokens: non_negative(statement.read::<Option<i64>, _>(6).ok().flatten()),
            cost_usd: statement.read::<Option<f64>, _>(7).ok().flatten(),
            timestamp_ms,
        });
    }
    frames.sort_by_key(|frame| frame.timestamp_ms);
    Ok(frames)
}

fn non_negative(value: Option<i64>) -> i64 {
    value.unwrap_or_default().max(0)
}

fn projects_table_is_usable(connection: &Connection) -> bool {
    let Ok(mut statement) = connection
        .prepare("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'projects'")
    else {
        return false;
    };
    if statement.next().ok() != Some(State::Row) {
        return false;
    }
    connection
        .prepare("SELECT id, name FROM projects LIMIT 1")
        .is_ok()
}

/// Strips routing prefixes such as "cs-switch-direct:" from model names.
fn normalize_model(model: &str) -> String {
    match model.split_once(':') {
        Some((prefix, rest)) if prefix.contains("cs-switch") => rest.to_string(),
        _ => model.to_string(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn strips_proxy_prefixes() {
        assert_eq!(
            super::normalize_model("cs-switch-direct:claude-sonnet-4-5"),
            "claude-sonnet-4-5"
        );
        assert_eq!(super::normalize_model("claude-sonnet-4-5"), "claude-sonnet-4-5");
    }
}
