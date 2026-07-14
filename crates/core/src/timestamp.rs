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

// ---- shifting a field under the cursor --------------------------------------

/// A shiftable field of a stamp. The weekday is not a field — a cursor there shifts the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Year,
    Month,
    Day,
    Hour,
    Minute,
    EndHour,
    EndMinute,
    RepeaterN,
    WarningN,
}

/// Which field the char offset `rel` (into the whole `<…>`/`[…]` text) sits in, if any.
/// Timestamps are ASCII, so char offsets equal byte offsets.
pub fn field_at(tsraw: &str, rel: usize) -> Option<Field> {
    let bytes = tsraw.as_bytes();
    if rel >= bytes.len() {
        return None;
    }
    // The date runs from index 1 to the first space (or the closing bracket).
    let date_end = tsraw[1..]
        .find([' ', '>', ']'])
        .map(|i| i + 1)
        .unwrap_or(bytes.len());
    // Date groups: year-month-day separated by '-'.
    let date = &tsraw[1..date_end];
    let mut dashes = date.match_indices('-').map(|(i, _)| i + 1);
    let d1 = dashes.next();
    let d2 = dashes.next();
    if let (Some(m0), Some(d0)) = (d1, d2) {
        if (1..1 + m0).contains(&rel) {
            return Some(Field::Year);
        }
        if (1 + m0..1 + d0).contains(&rel) {
            return Some(Field::Month);
        }
        if (1 + d0..date_end).contains(&rel) {
            return Some(Field::Day);
        }
    }
    // Tokens after the date, each at a known absolute offset.
    let mut o = date_end;
    for token in tsraw[date_end..bytes.len() - 1].split_inclusive(' ') {
        let start = o;
        o += token.len();
        let tok = token.trim_end_matches(' ');
        if tok.is_empty() {
            continue;
        }
        if !(start..o).contains(&rel) {
            continue;
        }
        let first = tok.as_bytes()[0];
        if tok.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Some(Field::Day); // the weekday word
        }
        if tok.contains(':') {
            let rel_in = rel - start;
            let (a, b) = match tok.split_once('-') {
                Some((a, b)) => (a, Some(b)),
                None => (tok, None),
            };
            if rel_in <= a.len() {
                // "HH:MM": before the colon is the hour, after is the minute.
                let colon = a.find(':').unwrap_or(a.len());
                return Some(if rel_in <= colon { Field::Hour } else { Field::Minute });
            } else if b.is_some() {
                let end_start = a.len() + 1;
                let end = b.unwrap();
                let colon = end.find(':').map(|c| c + end_start).unwrap_or(o);
                return Some(if rel_in <= colon { Field::EndHour } else { Field::EndMinute });
            }
        }
        if matches!(first, b'+' | b'.') {
            return Some(Field::RepeaterN);
        }
        if first == b'-' {
            return Some(Field::WarningN);
        }
    }
    None
}

/// Shift one `field` of `stamp` by one step up or down, with Org's rollover/clamp rules:
/// month and year clamp the day to the month length; day, hour, and minute carry.
pub fn shift_field(stamp: Stamp, field: Field, up: bool) -> Stamp {
    let d: i64 = if up { 1 } else { -1 };
    let mut s = stamp;
    match field {
        Field::Year => {
            s.date.year += d as i32;
            clamp_day(&mut s.date);
        }
        Field::Month => {
            let mut m = s.date.month as i64 - 1 + d;
            while m < 0 {
                m += 12;
                s.date.year -= 1;
            }
            while m > 11 {
                m -= 12;
                s.date.year += 1;
            }
            s.date.month = (m + 1) as u8;
            clamp_day(&mut s.date);
        }
        Field::Day => s.date = add_days(s.date, d),
        Field::Hour => shift_time(&mut s, false, d * 60),
        Field::Minute => shift_time(&mut s, false, d),
        Field::EndHour => shift_time(&mut s, true, d * 60),
        Field::EndMinute => shift_time(&mut s, true, d),
        Field::RepeaterN => {
            if let Some(r) = &mut s.repeater {
                r.n = (r.n as i64 + d).max(1) as u32;
            }
        }
        Field::WarningN => {
            if let Some(w) = &mut s.warning {
                w.n = (w.n as i64 + d).max(1) as u32;
            }
        }
    }
    s
}

fn clamp_day(date: &mut Date) {
    let max = days_in_month(date.year, date.month);
    if date.day > max {
        date.day = max;
    }
}

/// Add `delta` minutes to a stamp's `time` (or `time_end` when `end`), carrying into the date.
fn shift_time(stamp: &mut Stamp, end: bool, delta: i64) {
    let slot = if end { stamp.time_end } else { stamp.time };
    let Some(t) = slot else { return };
    let mut total = t.hour as i64 * 60 + t.min as i64 + delta;
    while total < 0 {
        total += 24 * 60;
        stamp.date = add_days(stamp.date, -1);
    }
    while total >= 24 * 60 {
        total -= 24 * 60;
        stamp.date = add_days(stamp.date, 1);
    }
    let new = Time {
        hour: (total / 60) as u8,
        min: (total % 60) as u8,
    };
    if end {
        stamp.time_end = Some(new);
    } else {
        stamp.time = Some(new);
    }
}

/// Add `delta` days to a date (proleptic Gregorian, via day ordinals).
fn add_days(date: Date, delta: i64) -> Date {
    civil_from_days(days_from_civil(date) + delta)
}

fn days_from_civil(date: Date) -> i64 {
    let y = date.year as i64 - (date.month <= 2) as i64;
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let m = date.month as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + date.day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> Date {
    let z = z + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    Date {
        year: (y + (month <= 2) as i64) as i32,
        month: month as u8,
        day: day as u8,
    }
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
