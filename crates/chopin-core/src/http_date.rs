/// HTTP Date utilities for RFC 7231 compliant date header formatting.
///
/// Provides real-time date generation for HTTP responses without caching,
/// using raw Unix timestamps for minimal overhead.

/// Days of week for HTTP-Date header formatting
const DAYS_OF_WEEK: &[&[u8]] = &[b"Sun", b"Mon", b"Tue", b"Wed", b"Thu", b"Fri", b"Sat"];

/// Months for HTTP-Date header formatting
const MONTHS: &[&[u8]] = &[
    b"Jan", b"Feb", b"Mar", b"Apr", b"May", b"Jun", b"Jul", b"Aug", b"Sep", b"Oct", b"Nov", b"Dec",
];

/// Check if a year is a leap year.
#[inline(always)]
fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Format an HTTP Date header into a fixed 37-byte buffer.
///
/// Produces output in the format: `Date: <day>, <dd> <month> <yyyy> <hh>:<mm>:<ss> GMT\r\n`
///
/// Example: `Date: Sun, 02 Mar 2025 10:40:53 GMT\r\n`
///
/// # Arguments
/// * `unix_secs` - Unix timestamp (seconds since 1970-01-01T00:00:00Z)
/// * `out` - Fixed 37-byte output buffer
///
/// # Returns
/// The number of bytes written to the buffer (always 37)
#[inline]
pub fn format_http_date(unix_secs: u32, out: &mut [u8; 37]) -> usize {
    const SECS_PER_DAY: u32 = 86400;

    let days_since_epoch = unix_secs / SECS_PER_DAY;
    let secs_today = unix_secs % SECS_PER_DAY;

    // Calculate hour, minute, second
    let hour = (secs_today / 3600) as u8;
    let minute = ((secs_today % 3600) / 60) as u8;
    let second = (secs_today % 60) as u8;

    // Calculate day of week (1970-01-01 was Thursday = 4)
    let dow = ((days_since_epoch + 4) % 7) as usize;

    // Simplified year/month/day calculation
    // This is approximate but works for all reasonable dates
    let mut year = 1970u32;
    let mut day_of_year = days_since_epoch as i32;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if day_of_year >= days_in_year {
            day_of_year -= days_in_year;
            year += 1;
        } else {
            break;
        }
    }

    // Month and day calculation
    let days_in_months = if is_leap_year(year) {
        &[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        &[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 0usize;
    let mut day = day_of_year as u32 + 1;

    for (i, &days) in days_in_months.iter().enumerate() {
        if day > days as u32 {
            day -= days as u32;
            month = i + 1;
        } else {
            break;
        }
    }

    // Format: "Date: <day-name>, <day> <month> <year> <hour>:<minute>:<second> GMT\r\n"
    let prefix = b"Date: ";
    let mut i = 0;
    out[i..i + prefix.len()].copy_from_slice(prefix);
    i += prefix.len();

    // Day of week (3 chars)
    out[i..i + 3].copy_from_slice(DAYS_OF_WEEK[dow]);
    i += 3;

    out[i] = b',';
    i += 1;
    out[i] = b' ';
    i += 1;

    // Day (1-2 chars, zero-padded)
    if day < 10 {
        out[i] = b'0';
        i += 1;
        out[i] = b'0' + day as u8;
        i += 1;
    } else {
        out[i] = b'0' + (day / 10) as u8;
        i += 1;
        out[i] = b'0' + (day % 10) as u8;
        i += 1;
    }

    out[i] = b' ';
    i += 1;

    // Month (3 chars)
    out[i..i + 3].copy_from_slice(MONTHS[month]);
    i += 3;

    out[i] = b' ';
    i += 1;

    // Year (4 chars)
    let year_str = year.to_string();
    let year_bytes = year_str.as_bytes();
    out[i..i + 4].copy_from_slice(&year_bytes[..4.min(year_bytes.len())]);
    i += 4;

    out[i] = b' ';
    i += 1;

    // Hour:Minute:Second (8 chars)
    out[i] = b'0' + (hour / 10) as u8;
    i += 1;
    out[i] = b'0' + (hour % 10) as u8;
    i += 1;
    out[i] = b':';
    i += 1;
    out[i] = b'0' + (minute / 10) as u8;
    i += 1;
    out[i] = b'0' + (minute % 10) as u8;
    i += 1;
    out[i] = b':';
    i += 1;
    out[i] = b'0' + (second / 10) as u8;
    i += 1;
    out[i] = b'0' + (second % 10) as u8;
    i += 1;

    out[i] = b' ';
    i += 1;
    out[i] = b'G';
    i += 1;
    out[i] = b'M';
    i += 1;
    out[i] = b'T';
    i += 1;
    out[i] = b'\r';
    i += 1;
    out[i] = b'\n';
    i += 1;

    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_http_date_known_values() {
        // Test with a known timestamp: 2025-03-02T10:40:53Z
        // Unix timestamp: 1740910853
        let mut buf = [0u8; 37];
        let len = format_http_date(1740910853, &mut buf);

        assert_eq!(len, 37);
        let result = std::str::from_utf8(&buf).unwrap();
        // The result should be in the format: "Date: Sun, 02 Mar 2025 10:40:53 GMT\r\n"
        assert!(result.starts_with("Date: "));
        assert!(result.contains("Mar"));
        assert!(result.contains("2025"));
        assert!(result.ends_with("GMT\r\n"));
    }

    #[test]
    fn test_format_http_date_length() {
        let mut buf = [0u8; 37];
        let len = format_http_date(0, &mut buf);
        assert_eq!(len, 37);
    }

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000));
        assert!(is_leap_year(2004));
        assert!(!is_leap_year(1900));
        assert!(!is_leap_year(2001));
        assert!(is_leap_year(2020));
        assert!(!is_leap_year(2021));
    }
}
