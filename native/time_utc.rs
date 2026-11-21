//! Minimal UTC timestamp helpers backed by `jiff` (zero timezone DB required).
use jiff::{SignedDuration, Timestamp, civil::date, tz::Offset};

pub fn now() -> Timestamp {
    Timestamp::now()
}

pub fn now_millis() -> i64 {
    Timestamp::now().as_millisecond()
}

pub fn rfc3339_millis() -> String {
    format_rfc3339_millis(Timestamp::now())
}

pub fn format_rfc3339_millis(ts: Timestamp) -> String {
    let dt = Offset::UTC.to_datetime(ts);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second(),
        dt.subsec_nanosecond() / 1_000_000
    )
}

pub fn rfc3339_seconds() -> String {
    format_rfc3339_seconds(Timestamp::now())
}

pub fn format_rfc3339_seconds(ts: Timestamp) -> String {
    let dt = Offset::UTC.to_datetime(ts);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

pub fn date_ymd() -> String {
    let dt = Offset::UTC.to_datetime(Timestamp::now());
    format!("{:04}-{:02}-{:02}", dt.year(), dt.month(), dt.day())
}

pub fn log_file_stamp() -> String {
    let dt = Offset::UTC.to_datetime(Timestamp::now());
    format!(
        "{:04}-{:02}-{:02}_{:02}-{:02}-{:02}",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

pub fn format_x509_utc(ts: Timestamp) -> String {
    let dt = Offset::UTC.to_datetime(ts);
    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}Z",
        dt.year(),
        dt.month(),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    )
}

pub fn parse_rfc3339(text: &str) -> Option<Timestamp> {
    text.parse::<Timestamp>().ok()
}

pub fn from_ymd_hms(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
) -> Option<Timestamp> {
    let year = i16::try_from(year).ok()?;
    let month = i8::try_from(month).ok()?;
    let day = i8::try_from(day).ok()?;
    let hour = i8::try_from(hour).ok()?;
    let minute = i8::try_from(minute).ok()?;
    let second = i8::try_from(second).ok()?;
    let dt = date(year, month, day).at(hour, minute, second, 0);
    Offset::UTC.to_timestamp(dt).ok()
}

pub fn sub_minutes(ts: Timestamp, minutes: i64) -> Option<Timestamp> {
    ts.checked_sub(SignedDuration::from_mins(minutes)).ok()
}

pub fn plus_one_year(ts: Timestamp) -> Option<Timestamp> {
    let dt = Offset::UTC.to_datetime(ts);
    let next_year = dt.year().checked_add(1)?;
    let day = if dt.month() == 2 && dt.day() == 29 {
        28
    } else {
        dt.day()
    };
    let next = date(next_year, dt.month(), day).at(
        dt.hour(),
        dt.minute(),
        dt.second(),
        dt.subsec_nanosecond(),
    );
    Offset::UTC.to_timestamp(next).ok()
}
