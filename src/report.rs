//! Aggregation and rendering of usage reports.

use std::collections::BTreeMap;

use crate::loader::Frame;

use crate::Report;

/// One aggregated report row.
#[derive(Debug, serde::Serialize)]
pub struct Row {
    pub period: String,
    pub models: Vec<String>,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub total_tokens: i64,
    pub cost_usd: f64,
    pub sessions: Option<usize>,
}

/// Groups frames into report rows.
pub fn aggregate(
    frames: &[Frame],
    report: Report,
    since: Option<&str>,
    until: Option<&str>,
    timezone: &jiff::tz::TimeZone,
) -> Result<Vec<Row>, String> {
    let since = match since {
        Some(since) => Some(parse_date(since)?),
        None => None,
    };
    let until = match until {
        Some(until) => Some(parse_date(until)?),
        None => None,
    };

    struct Bucket {
        models: BTreeMap<String, ()>,
        input_tokens: i64,
        output_tokens: i64,
        cache_read_tokens: i64,
        cache_write_tokens: i64,
        cost_usd: f64,
    }
    impl Bucket {
        fn add(&mut self, frame: &Frame) {
            self.models.entry(frame.model.clone()).or_insert(());
            self.input_tokens += frame.input_tokens;
            self.output_tokens += frame.output_tokens;
            self.cache_read_tokens += frame.cache_read_tokens;
            self.cache_write_tokens += frame.cache_write_tokens;
            self.cost_usd += frame.cost_usd.unwrap_or_default();
        }
        fn into_row(self, period: String, sessions: Option<usize>) -> Row {
            Row {
                period,
                models: self.models.into_keys().collect(),
                input_tokens: self.input_tokens,
                output_tokens: self.output_tokens,
                cache_read_tokens: self.cache_read_tokens,
                cache_creation_tokens: self.cache_write_tokens,
                total_tokens: self.input_tokens
                    + self.output_tokens
                    + self.cache_read_tokens
                    + self.cache_write_tokens,
                cost_usd: self.cost_usd,
                sessions,
            }
        }
    }

    let mut buckets: BTreeMap<String, Bucket> = BTreeMap::new();
    let mut frame_count: BTreeMap<String, usize> = BTreeMap::new();
    for frame in frames {
        let timestamp = jiff::Timestamp::from_millisecond(frame.timestamp_ms)
            .map_err(|_| "invalid frame timestamp".to_string())?
            .to_zoned(timezone.clone());
        let date = timestamp.date();
        if let Some(since) = &since {
            if date < *since {
                continue;
            }
        }
        if let Some(until) = &until {
            if date > *until {
                continue;
            }
        }
        let period = match report {
            Report::Daily => date.to_string(),
            Report::Monthly => format!("{}-{:02}", date.year(), date.month()),
            Report::Session => frame.session_id.clone(),
        };
        frame_count
            .entry(period.clone())
            .and_modify(|count| *count += 1)
            .or_insert(1);
        buckets
            .entry(period)
            .or_insert(Bucket {
                models: BTreeMap::new(),
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_write_tokens: 0,
                cost_usd: 0.0,
            })
            .add(frame);
    }
    Ok(buckets
        .into_iter()
        .map(|(period, bucket)| {
            let sessions = matches!(report, Report::Session).then_some(frame_count[&period]);
            bucket.into_row(period, sessions)
        })
        .collect())
}

fn parse_date(date: &str) -> Result<jiff::civil::Date, String> {
    date.parse()
        .map_err(|_| format!("invalid date '{date}', expected YYYY-MM-DD"))
}

pub fn parse_timezone(timezone: Option<&str>) -> Result<jiff::tz::TimeZone, String> {
    let name = timezone.unwrap_or("UTC");
    jiff::tz::TimeZone::get(name).map_err(|_| format!("unknown timezone '{name}'"))
}

pub fn print_json(rows: &[Row]) {
    let json = serde_json::to_string_pretty(rows).unwrap();
    println!("{json}");
}

pub fn print_table(rows: &[Row], report: Report) {
    let title = match report {
        Report::Daily => "Claude Science Daily Usage",
        Report::Monthly => "Claude Science Monthly Usage",
        Report::Session => "Claude Science Session Usage",
    };
    println!("{title}");
    println!("{}\n", "-".repeat(title.len()));
    println!(
        "{:<38} {:>12} {:>12} {:>12} {:>13} {:>13} {:>10}",
        "Date", "Input", "Output", "Cache Read", "Cache Create", "Total Tokens", "Cost ($)"
    );
    for row in rows {
        let models = format_models(&row.models);
        println!(
            "{:<38} {:>12} {:>12} {:>12} {:>13} {:>13} {:>10.4}",
            format!("{}{}", row.period, models),
            row.input_tokens,
            row.output_tokens,
            row.cache_read_tokens,
            row.cache_creation_tokens,
            row.total_tokens,
            row.cost_usd,
        );
    }
    if rows.is_empty() {
        println!("(no usage recorded)");
    }
}

fn format_models(models: &[String]) -> String {
    if models.is_empty() {
        String::new()
    } else {
        format!(" - {}", models.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loader::Frame;

    fn frame(model: &str, cost: Option<f64>, timestamp_ms: i64) -> Frame {
        Frame {
            session_id: "session-1".to_string(),
            model: model.to_string(),
            input_tokens: 100,
            output_tokens: 10,
            cache_read_tokens: 5,
            cache_write_tokens: 0,
            cost_usd: cost,
            timestamp_ms,
        }
    }

    #[test]
    fn daily_rows_sum_frames() {
        let frames = vec![
            frame("claude-sonnet-4-5", Some(0.5), 0),
            frame("claude-opus-4-6", None, 86_400_000),
        ];
        let rows = aggregate(&frames, Report::Daily, None, None, &parse_timezone(None).unwrap()).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].input_tokens, 100);
        assert_eq!(rows[0].models, vec!["claude-sonnet-4-5"]);
        assert!((rows[0].cost_usd - 0.5).abs() < 1e-9);
        assert_eq!(rows[1].cost_usd, 0.0);
    }

    #[test]
    fn date_bounds_filter_rows() {
        let frames = vec![
            frame("claude-sonnet-4-5", Some(0.5), 0),
            frame("claude-opus-4-6", None, 86_400_000),
        ];
        let timezone = parse_timezone(None).unwrap();
        let rows = aggregate(&frames, Report::Daily, Some("1970-01-02"), None, &timezone).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].period, "1970-01-02");
    }

    #[test]
    fn sessions_roll_up_by_root_frame() {
        let frames = vec![
            frame("claude-sonnet-4-5", Some(0.5), 0),
            frame("claude-opus-4-6", None, 86_400_000),
        ];
        let timezone = parse_timezone(None).unwrap();
        let rows = aggregate(&frames, Report::Session, None, None, &timezone).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].sessions, Some(2));
        assert_eq!(rows[0].input_tokens, 200);
    }
}