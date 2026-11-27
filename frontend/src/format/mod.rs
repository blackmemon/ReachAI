//! Dates and numbers for display. Times are stored in UTC and shown in the device time zone.

const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// "2026-11-21T16:05:00Z" shown as "21 Nov 2026, 21:05" for a device that is `offset_minutes` ahead of UTC.
pub fn local_time(utc: &str, offset_minutes: i32) -> String {
    let b = utc.as_bytes();
    if b.len() < 16 || b[4] != b'-' || b[7] != b'-' {
        return utc.to_string();
    }
    let n = |a: usize, z: usize| utc.get(a..z).and_then(|s| s.parse::<i64>().ok());
    let (Some(y), Some(mo), Some(d), Some(h), Some(mi)) = (n(0, 4), n(5, 7), n(8, 10), n(11, 13), n(14, 16)) else {
        return utc.to_string();
    };
    let mins = days_from_civil(y, mo, d) * 1440 + h * 60 + mi + offset_minutes as i64;
    let (days, rem) = (mins.div_euclid(1440), mins.rem_euclid(1440));
    let (yy, mm, dd) = civil_from_days(days);
    format!("{dd} {} {yy}, {:02}:{:02}", MONTHS[(mm - 1) as usize], rem / 60, rem % 60)
}

pub fn ago(seconds: i64) -> String {
    match seconds {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86400 => format!("{} h ago", s / 3600),
        s => format!("{} days ago", s / 86400),
    }
}

pub fn clock(seconds: i64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_device_time_zone() {
        assert_eq!(local_time("2026-11-21T16:05:00Z", 300), "21 Nov 2026, 21:05", "Karachi is UTC+5");
        assert_eq!(local_time("2026-11-21T22:30:00Z", 300), "22 Nov 2026, 03:30", "date rolls over");
        assert_eq!(local_time("2026-03-01T00:10:00Z", -60), "28 Feb 2026, 23:10");
        assert_eq!(local_time("2028-02-29T12:00:00Z", 0), "29 Feb 2028, 12:00", "leap day");
    }

    #[test]
    fn bad_text_is_returned_unchanged() {
        assert_eq!(local_time("soon", 300), "soon");
        assert_eq!(local_time("", 0), "");
    }

    #[test]
    fn ago_is_plain() {
        assert_eq!(ago(30), "just now");
        assert_eq!(ago(300), "5 min ago");
        assert_eq!(ago(7200), "2 h ago");
        assert_eq!(ago(172800), "2 days ago");
    }
}
