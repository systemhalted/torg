//! Org timestamps parsed as data — active `<2024-01-15 Mon 09:30>` and inactive `[…]`
//! stamps, optional time or time-range, `--` date ranges, `+1w`/`++2d`/`.+1m` repeaters and
//! `-2d` warnings. The parser tolerates a weekday on input and drops it; [`Display`] writes
//! the canonical form with the weekday recomputed.

use std::fmt;

/// A calendar date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

/// A wall-clock time (24-hour).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    pub hour: u8,
    pub min: u8,
}

/// A repeater/warning interval unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Hour,
    Day,
    Week,
    Month,
    Year,
}

/// A repeater's flavour: `+` (cumulate), `++` (catch-up), `.+` (restart-from-today).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepeatKind {
    Plus,
    PlusPlus,
    DotPlus,
}

/// A repeater cookie like `+1w`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Repeater {
    pub kind: RepeatKind,
    pub n: u32,
    pub unit: Unit,
}

/// A warning/delay cookie like `-2d` (or `--2d`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Warning {
    pub double: bool,
    pub n: u32,
    pub unit: Unit,
}

/// One `<…>` (active) or `[…]` (inactive) stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub active: bool,
    pub date: Date,
    pub time: Option<Time>,
    pub time_end: Option<Time>,
    pub repeater: Option<Repeater>,
    pub warning: Option<Warning>,
}

/// A timestamp: one stamp, or a `<a>--<b>` range of two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    pub start: Stamp,
    pub end: Option<Stamp>,
}

// ---- date arithmetic --------------------------------------------------------

/// Whether `year` is a Gregorian leap year.
pub fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// The number of days in `month` (1–12) of `year`.
pub fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// The three-letter English weekday for `date`, via Sakamoto's algorithm.
pub fn weekday(date: Date) -> &'static str {
    const NAMES: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mut y = date.year;
    let m = date.month as i32;
    if m < 3 {
        y -= 1;
    }
    let idx = (y + y / 4 - y / 100 + y / 400 + T[(m - 1) as usize] + date.day as i32).rem_euclid(7);
    NAMES[idx as usize]
}

// ---- Display ----------------------------------------------------------------

impl fmt::Display for Time {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.hour, self.min)
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let c = match self {
            Unit::Hour => 'h',
            Unit::Day => 'd',
            Unit::Week => 'w',
            Unit::Month => 'm',
            Unit::Year => 'y',
        };
        f.write_str(&c.to_string())
    }
}

impl fmt::Display for Stamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (open, close) = if self.active { ('<', '>') } else { ('[', ']') };
        write!(
            f,
            "{open}{:04}-{:02}-{:02} {}",
            self.date.year,
            self.date.month,
            self.date.day,
            weekday(self.date),
        )?;
        if let Some(t) = self.time {
            write!(f, " {t}")?;
            if let Some(e) = self.time_end {
                write!(f, "-{e}")?;
            }
        }
        if let Some(r) = self.repeater {
            let mark = match r.kind {
                RepeatKind::Plus => "+",
                RepeatKind::PlusPlus => "++",
                RepeatKind::DotPlus => ".+",
            };
            write!(f, " {mark}{}{}", r.n, r.unit)?;
        }
        if let Some(w) = self.warning {
            let mark = if w.double { "--" } else { "-" };
            write!(f, " {mark}{}{}", w.n, w.unit)?;
        }
        write!(f, "{close}")
    }
}

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.start)?;
        if let Some(end) = self.end {
            write!(f, "--{end}")?;
        }
        Ok(())
    }
}

// ---- parsing ----------------------------------------------------------------

/// Parse one timestamp at the start of `s`, returning it and the byte length consumed, or
/// `None` if `s` does not begin with a valid stamp.
pub fn parse_timestamp(s: &str) -> Option<(Timestamp, usize)> {
    let (start, len) = parse_stamp(s)?;
    // A `--` separator followed by a second stamp with matching brackets makes a range.
    if let Some(rest) = s.get(len..).and_then(|r| r.strip_prefix("--")) {
        if let Some((end, end_len)) = parse_stamp(rest) {
            if end.active == start.active {
                return Some((
                    Timestamp { start, end: Some(end) },
                    len + 2 + end_len,
                ));
            }
        }
    }
    Some((Timestamp { start, end: None }, len))
}

/// Byte ranges of every timestamp in `text`, left to right and non-overlapping.
pub fn find_timestamps(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' || bytes[i] == b'[' {
            if let Some((_, len)) = parse_timestamp(&text[i..]) {
                out.push((i, i + len));
                i += len;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Parse a single `<…>`/`[…]` stamp at the start of `s`.
fn parse_stamp(s: &str) -> Option<(Stamp, usize)> {
    let bytes = s.as_bytes();
    let active = match bytes.first()? {
        b'<' => true,
        b'[' => false,
        _ => return None,
    };
    let close = if active { b'>' } else { b']' };
    let end = bytes.iter().position(|&b| b == close)?;
    let inner = &s[1..end];
    let stamp = parse_stamp_inner(inner, active)?;
    Some((stamp, end + 1))
}

/// Parse the content between the brackets: `date [dow] [time[-time]] [repeater] [warning]`.
fn parse_stamp_inner(inner: &str, active: bool) -> Option<Stamp> {
    let mut parts = inner.split_whitespace();
    let date = parse_date(parts.next()?)?;
    let mut stamp = Stamp {
        active,
        date,
        time: None,
        time_end: None,
        repeater: None,
        warning: None,
    };
    for part in parts {
        let first = part.as_bytes()[0];
        if first.is_ascii_digit() && part.contains(':') {
            let (t, e) = parse_time_field(part)?;
            stamp.time = Some(t);
            stamp.time_end = e;
        } else if matches!(first, b'+' | b'.') {
            stamp.repeater = Some(parse_repeater(part)?);
        } else if first == b'-' {
            stamp.warning = Some(parse_warning(part)?);
        } else {
            // A weekday word (or any other non-cookie token) — tolerated and ignored.
            if part.bytes().all(|b| b.is_ascii_alphabetic()) {
                continue;
            }
            return None;
        }
    }
    Some(stamp)
}

fn parse_date(s: &str) -> Option<Date> {
    let mut it = s.splitn(3, '-');
    let year: i32 = it.next()?.parse().ok()?;
    let month: u8 = it.next()?.parse().ok()?;
    let day: u8 = it.next()?.parse().ok()?;
    if it.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    if day < 1 || day > days_in_month(year, month) {
        return None;
    }
    Some(Date { year, month, day })
}

fn parse_time(s: &str) -> Option<Time> {
    let (h, m) = s.split_once(':')?;
    let hour: u8 = h.parse().ok()?;
    let min: u8 = m.parse().ok()?;
    if hour > 23 || min > 59 {
        return None;
    }
    Some(Time { hour, min })
}

fn parse_time_field(s: &str) -> Option<(Time, Option<Time>)> {
    match s.split_once('-') {
        Some((a, b)) => Some((parse_time(a)?, Some(parse_time(b)?))),
        None => Some((parse_time(s)?, None)),
    }
}

fn parse_unit(c: u8) -> Option<Unit> {
    match c {
        b'h' => Some(Unit::Hour),
        b'd' => Some(Unit::Day),
        b'w' => Some(Unit::Week),
        b'm' => Some(Unit::Month),
        b'y' => Some(Unit::Year),
        _ => None,
    }
}

fn parse_repeater(s: &str) -> Option<Repeater> {
    let (kind, rest) = if let Some(r) = s.strip_prefix("++") {
        (RepeatKind::PlusPlus, r)
    } else if let Some(r) = s.strip_prefix(".+") {
        (RepeatKind::DotPlus, r)
    } else if let Some(r) = s.strip_prefix('+') {
        (RepeatKind::Plus, r)
    } else {
        return None;
    };
    let (n, unit) = parse_count_unit(rest)?;
    Some(Repeater { kind, n, unit })
}

fn parse_warning(s: &str) -> Option<Warning> {
    let (double, rest) = if let Some(r) = s.strip_prefix("--") {
        (true, r)
    } else if let Some(r) = s.strip_prefix('-') {
        (false, r)
    } else {
        return None;
    };
    let (n, unit) = parse_count_unit(rest)?;
    Some(Warning { double, n, unit })
}

/// Split a `<digits><unit>` cookie tail like `1w`.
fn parse_count_unit(s: &str) -> Option<(u32, Unit)> {
    let bytes = s.as_bytes();
    let split = bytes.iter().position(|b| !b.is_ascii_digit())?;
    if split == 0 || split != bytes.len() - 1 {
        return None;
    }
    let n: u32 = s[..split].parse().ok()?;
    let unit = parse_unit(bytes[split])?;
    Some((n, unit))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- date arithmetic ----------------------------------------------------

    #[test]
    fn leap_years() {
        assert!(is_leap(2024));
        assert!(is_leap(2000));
        assert!(!is_leap(2023));
        assert!(!is_leap(1900));
    }

    #[test]
    fn month_lengths() {
        assert_eq!(days_in_month(2024, 2), 29);
        assert_eq!(days_in_month(2023, 2), 28);
        assert_eq!(days_in_month(2024, 4), 30);
        assert_eq!(days_in_month(2024, 12), 31);
    }

    #[test]
    fn weekday_matches_known_dates() {
        assert_eq!(weekday(Date { year: 2024, month: 1, day: 15 }), "Mon");
        assert_eq!(weekday(Date { year: 2024, month: 1, day: 16 }), "Tue");
        assert_eq!(weekday(Date { year: 2000, month: 1, day: 1 }), "Sat");
    }

    // ---- Display round-trip -------------------------------------------------

    fn ts(s: &str) -> Timestamp {
        parse_timestamp(s).expect("parse").0
    }

    #[test]
    fn display_adds_the_weekday() {
        assert_eq!(ts("<2024-01-15>").to_string(), "<2024-01-15 Mon>");
        assert_eq!(ts("[2024-01-15]").to_string(), "[2024-01-15 Mon]");
    }

    #[test]
    fn round_trips_time_and_range() {
        assert_eq!(ts("<2024-01-15 09:30>").to_string(), "<2024-01-15 Mon 09:30>");
        assert_eq!(
            ts("<2024-01-15 09:30-11:00>").to_string(),
            "<2024-01-15 Mon 09:30-11:00>"
        );
    }

    #[test]
    fn round_trips_date_range_repeater_and_warning() {
        assert_eq!(
            ts("<2024-01-15>--<2024-01-18>").to_string(),
            "<2024-01-15 Mon>--<2024-01-18 Thu>"
        );
        assert_eq!(ts("<2024-01-15 +1w>").to_string(), "<2024-01-15 Mon +1w>");
        assert_eq!(ts("<2024-01-15 ++2d>").to_string(), "<2024-01-15 Mon ++2d>");
        assert_eq!(ts("<2024-01-15 .+1m>").to_string(), "<2024-01-15 Mon .+1m>");
        assert_eq!(
            ts("<2024-01-15 09:30 +1w -2d>").to_string(),
            "<2024-01-15 Mon 09:30 +1w -2d>"
        );
    }

    #[test]
    fn tolerates_an_input_weekday() {
        assert_eq!(ts("<2024-01-15 Zzz>").to_string(), "<2024-01-15 Mon>");
    }

    // ---- parse rejection ----------------------------------------------------

    #[test]
    fn rejects_malformed_stamps() {
        assert!(parse_timestamp("<2024-13-01>").is_none()); // bad month
        assert!(parse_timestamp("<2024-01-40>").is_none()); // bad day
        assert!(parse_timestamp("<2024-01-15").is_none()); // unclosed
        assert!(parse_timestamp("plain text").is_none());
        assert!(parse_timestamp("<2024-02-30>").is_none()); // day past month length
    }

    #[test]
    fn parse_reports_consumed_length() {
        let (_, len) = parse_timestamp("<2024-01-15> trailing").unwrap();
        assert_eq!(len, "<2024-01-15>".len());
        // A date range consumes both stamps and the separator.
        let (_, len) = parse_timestamp("<2024-01-15>--<2024-01-18> x").unwrap();
        assert_eq!(len, "<2024-01-15>--<2024-01-18>".len());
    }

    // ---- find_timestamps ----------------------------------------------------

    #[test]
    fn finds_timestamps_in_a_line() {
        let line = "meet <2024-01-15 Mon> and [2024-02-01]";
        let spans = find_timestamps(line);
        assert_eq!(spans.len(), 2);
        assert_eq!(&line[spans[0].0..spans[0].1], "<2024-01-15 Mon>");
        assert_eq!(&line[spans[1].0..spans[1].1], "[2024-02-01]");
    }

    #[test]
    fn find_skips_non_timestamps() {
        assert!(find_timestamps("a < b > c").is_empty());
        assert!(find_timestamps("no brackets here").is_empty());
    }
}
