//! Calendar units and the ISO week date, in the `chrono`-compatible shape.
//!
//! [`Days`] and [`Months`] are non-negative, type-safe calendar offsets.
//! Wrapping them in distinct types means a day count can never be silently
//! mistaken for a month count — the same class of unit confusion `chrono`
//! eliminates with its `Days` / `Months` newtypes. [`IsoWeek`] is the ISO
//! 8601 week date `(iso_year, week)`.

use core::fmt;

use crate::calendar::{days_from_civil, iso_week_from_civil, weekday_from_civil, Weekday};
use crate::date::Date;
use crate::error::{Error, Result};

/// A non-negative number of calendar days.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Days(pub u64);

impl Days {
    /// Build from a day count.
    pub const fn new(days: u64) -> Days {
        Days(days)
    }

    /// The inner day count.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A non-negative number of calendar months.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Months(pub u32);

impl Months {
    /// Build from a month count.
    pub const fn new(months: u32) -> Months {
        Months(months)
    }

    /// The inner month count.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// An ISO 8601 week date: an ISO year and a week number in `1..=53`.
///
/// The ISO year can differ from the calendar year for the first days of
/// January and the last days of December (e.g. `2021-01-01` is ISO week
/// `2020-W53`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IsoWeek {
    year: i32,
    week: u32,
}

impl IsoWeek {
    pub(crate) const fn new(year: i32, week: u32) -> IsoWeek {
        IsoWeek { year, week }
    }

    /// The ISO year.
    pub const fn year(self) -> i32 {
        self.year
    }

    /// The ISO week number (1-based).
    pub const fn week(self) -> u32 {
        self.week
    }

    /// `(iso_year, week)`.
    pub const fn parts(self) -> (i32, u32) {
        (self.year, self.week)
    }

    /// The Monday that starts this week, as a calendar date.
    ///
    /// The resulting date lives in the ISO year (which may differ from the
    /// calendar year at the boundaries).
    pub fn monday(self) -> Result<Date> {
        let max_week = iso_week_from_civil(self.year, 12, 28).1;
        if self.week == 0 || self.week > max_week {
            return Err(Error::out_of_range("iso week"));
        }
        // Week 1 of the ISO year is the week containing January 4.
        let jan4 = days_from_civil(self.year, 1, 4);
        let monday_week1 = jan4 - weekday_from_civil(jan4) as i64;
        Date::from_days_checked(monday_week1 + (self.week as i64 - 1) * 7)
    }

    /// The weekday of the Monday of this week (always the week's first day).
    pub const fn first_weekday(self) -> Weekday {
        Weekday::Monday
    }
}

impl fmt::Display for IsoWeek {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-W{:02}", self.year, self.week)
    }
}

#[cfg(all(test, feature = "alloc"))]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn iso_week_monday() {
        let w = IsoWeek::new(2021, 1);
        assert_eq!(w.monday().unwrap(), Date::from_ymd(2021, 1, 4).unwrap());
        let w = IsoWeek::new(2020, 53);
        assert_eq!(w.monday().unwrap(), Date::from_ymd(2020, 12, 28).unwrap());
        let w = IsoWeek::new(2026, 1);
        assert_eq!(w.monday().unwrap(), Date::from_ymd(2025, 12, 29).unwrap());
        assert_eq!(w.to_string(), "2026-W01");
        assert!(IsoWeek::new(2024, 54).monday().is_err());
        assert!(IsoWeek::new(2021, 53).monday().is_err());
    }
}
