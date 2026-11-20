//! Daily log files under `%APPDATA%\ADBHelper\logs`, kept for a fixed number of days.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const PREFIX: &str = "adbhelper-";
const SUFFIX: &str = ".log";
pub const KEEP_DAYS: i64 = 7;

pub fn directory() -> io::Result<PathBuf> {
    crate::settings::file("logs")
}

/// Calendar date and wall-clock time used for file names and line stamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
}

impl Stamp {
    pub fn now() -> Self {
        #[cfg(windows)]
        {
            // Local time so the file names and stamps match the user's clock.
            let mut time = unsafe { std::mem::zeroed() };
            unsafe { windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut time) };
            Self {
                year: i32::from(time.wYear),
                month: u32::from(time.wMonth),
                day: u32::from(time.wDay),
                hour: u32::from(time.wHour),
                minute: u32::from(time.wMinute),
                second: u32::from(time.wSecond),
            }
        }
        #[cfg(not(windows))]
        {
            let seconds = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_secs() as i64)
                .unwrap_or_default();
            Self::from_unix(seconds)
        }
    }

    #[cfg_attr(windows, allow(dead_code))]
    pub fn from_unix(seconds: i64) -> Self {
        let days = seconds.div_euclid(86_400);
        let rest = seconds.rem_euclid(86_400) as u32;
        let (year, month, day) = civil_from_days(days);
        Self {
            year,
            month,
            day,
            hour: rest / 3600,
            minute: rest % 3600 / 60,
            second: rest % 60,
        }
    }

    fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    fn time(&self) -> String {
        format!("{:02}:{:02}:{:02}", self.hour, self.minute, self.second)
    }

    fn day_number(&self) -> i64 {
        days_from_civil(self.year, self.month, self.day)
    }
}

// Howard Hinnant's civil calendar conversions (proleptic Gregorian calendar).
fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_index + 2) / 5 + 1) as u32;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year as i32, month, day)
}

fn file_name(stamp: &Stamp) -> String {
    format!("{PREFIX}{}{SUFFIX}", stamp.date())
}

fn date_of(file_name: &str) -> Option<i64> {
    let date = file_name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    let mut parts = date.split('-');
    let year = parts.next()?.parse().ok()?;
    let month = parts
        .next()?
        .parse()
        .ok()
        .filter(|m| (1..=12).contains(m))?;
    let day = parts
        .next()?
        .parse()
        .ok()
        .filter(|d| (1..=31).contains(d))?;
    (parts.next().is_none() && date.len() == 10).then(|| days_from_civil(year, month, day))
}

/// Log file names that are older than `KEEP_DAYS` relative to `today`.
fn expired<'a>(names: impl IntoIterator<Item = &'a str>, today: &Stamp) -> Vec<&'a str> {
    let today = today.day_number();
    names
        .into_iter()
        .filter(|name| date_of(name).is_some_and(|day| today - day >= KEEP_DAYS))
        .collect()
}

/// Appends each line to the current day's file and removes expired files on day change.
pub struct LogFile {
    directory: Option<PathBuf>,
    day: Option<String>,
    file: Option<File>,
}

impl LogFile {
    pub fn open_default() -> Self {
        Self::in_directory(directory().ok())
    }

    pub fn in_directory(directory: Option<PathBuf>) -> Self {
        Self {
            directory,
            day: None,
            file: None,
        }
    }

    pub fn write(&mut self, message: &str) {
        self.write_at(&Stamp::now(), message);
    }

    pub fn write_at(&mut self, stamp: &Stamp, message: &str) {
        let Some(directory) = self.directory.clone() else {
            return;
        };
        let day = stamp.date();
        if self.day.as_deref() != Some(day.as_str()) {
            self.day = Some(day);
            self.file = open(&directory, stamp).ok();
            remove_expired(&directory, stamp);
        }
        if let Some(file) = &mut self.file {
            // Logging must never stop an action; a failed write only drops this line.
            let time = stamp.time();
            let mut line = String::with_capacity(message.len() + 12);
            for (index, part) in message.lines().enumerate() {
                line.push_str(if index == 0 { &time } else { "        " });
                line.push_str("  ");
                line.push_str(part);
                line.push_str("\r\n");
            }
            if line.is_empty() {
                line = format!("{}\r\n", stamp.time());
            }
            let _ = file.write_all(line.as_bytes());
        }
    }
}

fn open(directory: &Path, stamp: &Stamp) -> io::Result<File> {
    fs::create_dir_all(directory)?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join(file_name(stamp)))
}

fn remove_expired(directory: &Path, today: &Stamp) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    for name in expired(names.iter().map(String::as_str), today) {
        let _ = fs::remove_file(directory.join(name));
    }
}

/// Opens the log folder in Windows Explorer, creating it first if needed.
pub fn open_folder() -> Result<(), String> {
    let directory = directory().map_err(|error| error.to_string())?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    #[cfg(windows)]
    let program = "explorer";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(windows, target_os = "macos")))]
    let program = "xdg-open";
    std::process::Command::new(program)
        .arg(&directory)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(year: i32, month: u32, day: u32) -> Stamp {
        Stamp {
            year,
            month,
            day,
            hour: 8,
            minute: 5,
            second: 9,
        }
    }

    #[test]
    fn calendar_conversion_round_trips_and_matches_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(Stamp::from_unix(1_795_132_800).date(), "2026-11-20");
        assert_eq!(Stamp::from_unix(951_782_400).date(), "2000-02-29");
        for day in (-800_000..800_000).step_by(997) {
            let (year, month, date) = civil_from_days(day);
            assert_eq!(days_from_civil(year, month, date), day);
        }
    }

    #[test]
    fn only_own_files_older_than_the_retention_window_expire() {
        let names = [
            "adbhelper-2026-11-20.log",
            "adbhelper-2026-11-14.log",
            "adbhelper-2026-11-13.log",
            "adbhelper-2025-12-31.log",
            "adbhelper-2026-11-1.log",
            "adbhelper-2026-13-01.log",
            "notes-2026-01-01.log",
            "adbhelper-2026-01-01.txt",
        ];
        assert_eq!(
            expired(names, &stamp(2026, 11, 20)),
            ["adbhelper-2026-11-13.log", "adbhelper-2025-12-31.log"]
        );
    }

    #[test]
    fn lines_are_stamped_appended_and_rotated_by_day() {
        let directory = std::env::temp_dir().join(format!(
            "adbhelper-logfile-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("adbhelper-2026-11-01.log"), "old").unwrap();
        fs::write(directory.join("keep.txt"), "other").unwrap();

        let mut log = LogFile::in_directory(Some(directory.clone()));
        log.write_at(&stamp(2026, 11, 19), "first");
        log.write_at(&stamp(2026, 11, 19), "two\nlines");
        log.write_at(&stamp(2026, 11, 20), "next day");

        let first = fs::read_to_string(directory.join("adbhelper-2026-11-19.log")).unwrap();
        assert_eq!(
            first,
            "08:05:09  first\r\n08:05:09  two\r\n          lines\r\n"
        );
        let second = fs::read_to_string(directory.join("adbhelper-2026-11-20.log")).unwrap();
        assert_eq!(second, "08:05:09  next day\r\n");
        assert!(!directory.join("adbhelper-2026-11-01.log").exists());
        assert!(directory.join("keep.txt").exists());
        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn missing_directory_disables_logging_without_panicking() {
        let mut log = LogFile::in_directory(None);
        log.write_at(&stamp(2026, 11, 20), "ignored");
    }
}
