//! Explicit, deterministic civil-time and astronomy snapshots.
//!
//! This module owns environment interpretation only. It does not select a
//! scene, load assets, touch Wayland, or use the renderer's monotonic clock.
//! Location and timezone inputs are explicit; there is no ambient geolocation.

#![allow(dead_code)]

use std::{
    error::Error,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use chrono::{DateTime, Datelike, Offset, Timelike, Utc};
use chrono_tz::Tz;

const SECONDS_PER_DAY: i64 = 86_400;
const SECONDS_PER_MINUTE: i64 = 60;
const TWILIGHT_SECONDS: i64 = 30 * 60;
const SUNRISE_ZENITH_DEGREES: f64 = 90.833;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentError(String);

impl fmt::Display for EnvironmentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for EnvironmentError {}

fn error(message: impl Into<String>) -> EnvironmentError {
    EnvironmentError(message.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum TimeOfDay {
    Dawn,
    Day,
    Dusk,
    Night,
    PolarDay,
    PolarNight,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Weather {
    Clear,
    Cloudy,
    Rain,
    Snow,
    Storm,
    Fog,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Season {
    Spring,
    Summer,
    Autumn,
    Winter,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnvironmentSnapshot {
    pub time: TimeOfDay,
    pub weather: Weather,
    pub season: Season,
}

impl EnvironmentSnapshot {
    pub const fn unknown() -> Self {
        Self {
            time: TimeOfDay::Unknown,
            weather: Weather::Unknown,
            season: Season::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Location {
    latitude_degrees: f64,
    longitude_degrees: f64,
}

impl Location {
    pub fn new(latitude_degrees: f64, longitude_degrees: f64) -> Result<Self, EnvironmentError> {
        if !latitude_degrees.is_finite()
            || !longitude_degrees.is_finite()
            || !(-90.0..=90.0).contains(&latitude_degrees)
            || !(-180.0..=180.0).contains(&longitude_degrees)
        {
            return Err(error(
                "location coordinates are outside the permitted finite ranges",
            ));
        }
        Ok(Self {
            latitude_degrees,
            longitude_degrees,
        })
    }

    pub const fn latitude_degrees(self) -> f64 {
        self.latitude_degrees
    }

    pub const fn longitude_degrees(self) -> f64 {
        self.longitude_degrees
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OffsetTransition {
    pub at_utc_seconds: i64,
    pub offset_minutes: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TimeZoneRules {
    Schedule {
        initial_offset_minutes: i32,
        transitions: Vec<OffsetTransition>,
    },
    Iana(Tz),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UtcOffsetSchedule {
    rules: TimeZoneRules,
}

impl UtcOffsetSchedule {
    pub fn new(
        initial_offset_minutes: i32,
        transitions: Vec<OffsetTransition>,
    ) -> Result<Self, EnvironmentError> {
        validate_offset(initial_offset_minutes)?;
        let mut previous = None;
        for transition in &transitions {
            validate_offset(transition.offset_minutes)?;
            if previous.is_some_and(|at| transition.at_utc_seconds <= at) {
                return Err(error("timezone transitions must be strictly ordered"));
            }
            previous = Some(transition.at_utc_seconds);
        }
        Ok(Self {
            rules: TimeZoneRules::Schedule {
                initial_offset_minutes,
                transitions,
            },
        })
    }

    pub fn fixed(offset_minutes: i32) -> Result<Self, EnvironmentError> {
        validate_offset(offset_minutes)?;
        Ok(Self {
            rules: TimeZoneRules::Schedule {
                initial_offset_minutes: offset_minutes,
                transitions: Vec::new(),
            },
        })
    }

    /// Use a validated IANA name from the pinned chrono-tz database.
    pub fn iana(timezone: Tz) -> Self {
        Self {
            rules: TimeZoneRules::Iana(timezone),
        }
    }

    pub fn offset_at(&self, utc_seconds: i64) -> i32 {
        match &self.rules {
            TimeZoneRules::Schedule {
                initial_offset_minutes,
                transitions,
            } => transitions
                .iter()
                .take_while(|transition| transition.at_utc_seconds <= utc_seconds)
                .last()
                .map_or(*initial_offset_minutes, |transition| {
                    transition.offset_minutes
                }),
            TimeZoneRules::Iana(timezone) => DateTime::<Utc>::from_timestamp(utc_seconds, 0)
                .map(|instant| {
                    instant
                        .with_timezone(timezone)
                        .offset()
                        .fix()
                        .local_minus_utc()
                        / 60
                })
                .unwrap_or(0),
        }
    }

    pub fn local_datetime_at(&self, utc_seconds: i64) -> Result<CivilDateTime, EnvironmentError> {
        match &self.rules {
            TimeZoneRules::Schedule { .. } => {
                CivilDateTime::from_utc_seconds(utc_seconds, self.offset_at(utc_seconds))
            }
            TimeZoneRules::Iana(timezone) => {
                let instant = DateTime::<Utc>::from_timestamp(utc_seconds, 0)
                    .ok_or_else(|| error("UTC timestamp is outside chrono's supported range"))?;
                let local = instant.with_timezone(timezone);
                Ok(CivilDateTime {
                    date: CivilDate::new(local.year(), local.month() as u8, local.day() as u8)?,
                    hour: local.hour() as u8,
                    minute: local.minute() as u8,
                    second: local.second() as u8,
                })
            }
        }
    }
}

fn validate_offset(offset_minutes: i32) -> Result<(), EnvironmentError> {
    if (-14 * 60..=14 * 60).contains(&offset_minutes) {
        Ok(())
    } else {
        Err(error("UTC offset must be between -14:00 and +14:00"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CivilDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

impl CivilDate {
    pub fn new(year: i32, month: u8, day: u8) -> Result<Self, EnvironmentError> {
        let date = Self { year, month, day };
        if date.is_valid() {
            Ok(date)
        } else {
            Err(error("invalid civil calendar date"))
        }
    }

    pub fn is_valid(self) -> bool {
        (1..=12).contains(&self.month)
            && (1..=days_in_month(self.year, self.month)).contains(&self.day)
    }

    pub fn day_of_year(self) -> u16 {
        (1..self.month)
            .map(|month| days_in_month(self.year, month) as u16)
            .sum::<u16>()
            + u16::from(self.day)
    }

    pub fn unix_seconds_at_midnight(self) -> i64 {
        days_from_civil(self.year, self.month, self.day) * SECONDS_PER_DAY
    }

    pub fn add_days(self, days: i64) -> Result<Self, EnvironmentError> {
        let (year, month, day) =
            civil_from_days(days_from_civil(self.year, self.month, self.day) + days);
        Self::new(year, month, day)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CivilDateTime {
    pub date: CivilDate,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl CivilDateTime {
    pub fn from_utc_seconds(
        utc_seconds: i64,
        offset_minutes: i32,
    ) -> Result<Self, EnvironmentError> {
        let local_seconds = utc_seconds
            .checked_add(i64::from(offset_minutes) * SECONDS_PER_MINUTE)
            .ok_or_else(|| error("civil time overflow"))?;
        let days = local_seconds.div_euclid(SECONDS_PER_DAY);
        let seconds = local_seconds.rem_euclid(SECONDS_PER_DAY);
        let (year, month, day) = civil_from_days(days);
        Ok(Self {
            date: CivilDate::new(year, month, day)?,
            hour: (seconds / 3_600) as u8,
            minute: ((seconds % 3_600) / 60) as u8,
            second: (seconds % 60) as u8,
        })
    }
}

#[allow(dead_code)]
pub trait WallClock {
    fn now_utc_seconds(&self) -> Result<i64, EnvironmentError>;
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemWallClock;

#[allow(dead_code)]
impl WallClock for SystemWallClock {
    fn now_utc_seconds(&self) -> Result<i64, EnvironmentError> {
        match SystemTime::now().duration_since(UNIX_EPOCH) {
            Ok(duration) => i64::try_from(duration.as_secs())
                .map_err(|_| error("system clock is beyond supported Unix timestamp range")),
            Err(error_value) => i64::try_from(error_value.duration().as_secs())
                .map(|seconds| -seconds)
                .map_err(|_| error("system clock is before supported Unix timestamp range")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SolarEvents {
    pub sunrise_utc_seconds: Option<i64>,
    pub sunset_utc_seconds: Option<i64>,
    pub polar: Option<TimeOfDay>,
}

impl SolarEvents {
    pub const fn normal(sunrise_utc_seconds: i64, sunset_utc_seconds: i64) -> Self {
        Self {
            sunrise_utc_seconds: Some(sunrise_utc_seconds),
            sunset_utc_seconds: Some(sunset_utc_seconds),
            polar: None,
        }
    }

    pub const fn polar_day() -> Self {
        Self {
            sunrise_utc_seconds: None,
            sunset_utc_seconds: None,
            polar: Some(TimeOfDay::PolarDay),
        }
    }

    pub const fn polar_night() -> Self {
        Self {
            sunrise_utc_seconds: None,
            sunset_utc_seconds: None,
            polar: Some(TimeOfDay::PolarNight),
        }
    }

    pub const fn unavailable() -> Self {
        Self {
            sunrise_utc_seconds: None,
            sunset_utc_seconds: None,
            polar: None,
        }
    }
}

pub trait AstronomyProvider {
    fn solar_events(
        &self,
        date: CivilDate,
        location: Location,
    ) -> Result<SolarEvents, EnvironmentError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoaaAstronomy;

impl AstronomyProvider for NoaaAstronomy {
    fn solar_events(
        &self,
        date: CivilDate,
        location: Location,
    ) -> Result<SolarEvents, EnvironmentError> {
        let latitude = location.latitude_degrees().to_radians();
        let longitude = location.longitude_degrees();
        let day = f64::from(date.day_of_year());
        let longitude_hour = longitude / 15.0;
        let solar_declination = solar_declination(day);
        let cos_hour_angle = (SUNRISE_ZENITH_DEGREES.to_radians().cos()
            - latitude.sin() * solar_declination.sin())
            / (latitude.cos() * solar_declination.cos());

        if cos_hour_angle > 1.0 {
            return Ok(SolarEvents::polar_night());
        }
        if cos_hour_angle < -1.0 {
            return Ok(SolarEvents::polar_day());
        }

        let mut sunrise = solar_event_utc(date, longitude_hour, cos_hour_angle, true);
        let mut sunset = solar_event_utc(date, longitude_hour, cos_hour_angle, false);
        if sunset <= sunrise {
            // NOAA's formula returns a UTC hour in [0, 24). Choose the
            // adjacent UTC date that belongs to this location's local date.
            if longitude >= 0.0 {
                sunrise -= SECONDS_PER_DAY;
            } else {
                sunset += SECONDS_PER_DAY;
            }
        }
        Ok(SolarEvents::normal(sunrise, sunset))
    }
}

fn solar_declination(day_of_year: f64) -> f64 {
    let gamma = 2.0 * std::f64::consts::PI / 365.0 * (day_of_year - 1.0);
    0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin() - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin()
}

fn solar_event_utc(
    date: CivilDate,
    longitude_hour: f64,
    cos_hour_angle: f64,
    sunrise: bool,
) -> i64 {
    let day = f64::from(date.day_of_year());
    let approximate_time = day
        + if sunrise {
            (6.0 - longitude_hour) / 24.0
        } else {
            (18.0 - longitude_hour) / 24.0
        };
    let mean_anomaly = 0.9856 * approximate_time - 3.289;
    let true_longitude = normalize_degrees(
        mean_anomaly
            + 1.916 * mean_anomaly.to_radians().sin()
            + 0.020 * (2.0 * mean_anomaly).to_radians().sin()
            + 282.634,
    );
    let right_ascension = normalize_degrees(
        (0.91764 * true_longitude.to_radians().tan())
            .atan()
            .to_degrees(),
    );
    let longitude_quadrant = (true_longitude / 90.0).floor() * 90.0;
    let right_ascension_quadrant = (right_ascension / 90.0).floor() * 90.0;
    let right_ascension = (right_ascension + longitude_quadrant - right_ascension_quadrant) / 15.0;
    let local_hour_angle = (if sunrise {
        360.0 - cos_hour_angle.acos().to_degrees()
    } else {
        cos_hour_angle.acos().to_degrees()
    }) / 15.0;
    let local_mean_time = local_hour_angle + right_ascension - 0.06571 * approximate_time - 6.622;
    let utc_hour = normalize_hours(local_mean_time - longitude_hour);
    date.unix_seconds_at_midnight() + (utc_hour * 3_600.0).round() as i64
}

fn normalize_degrees(value: f64) -> f64 {
    value.rem_euclid(360.0)
}

fn normalize_hours(value: f64) -> f64 {
    value.rem_euclid(24.0)
}

#[derive(Debug, Clone)]
pub struct EnvironmentProvider<A> {
    pub location: Option<Location>,
    pub timezone: Option<UtcOffsetSchedule>,
    pub astronomy: A,
}

impl<A> EnvironmentProvider<A>
where
    A: AstronomyProvider,
{
    pub fn snapshot_at(&self, utc_seconds: i64, weather: Weather) -> EnvironmentSnapshot {
        let (Some(location), Some(timezone)) = (self.location, self.timezone.as_ref()) else {
            return EnvironmentSnapshot {
                time: TimeOfDay::Unknown,
                weather,
                season: Season::Unknown,
            };
        };
        let Ok(local) = timezone.local_datetime_at(utc_seconds) else {
            return EnvironmentSnapshot {
                time: TimeOfDay::Unknown,
                weather,
                season: Season::Unknown,
            };
        };
        let time = self
            .astronomy
            .solar_events(local.date, location)
            .map(|events| classify_time(utc_seconds, events))
            .unwrap_or(TimeOfDay::Unknown);
        EnvironmentSnapshot {
            time,
            weather,
            season: season_for(local.date, location),
        }
    }
}

pub fn classify_time(utc_seconds: i64, events: SolarEvents) -> TimeOfDay {
    if let Some(polar) = events.polar {
        return polar;
    }
    let (Some(sunrise), Some(sunset)) = (events.sunrise_utc_seconds, events.sunset_utc_seconds)
    else {
        return TimeOfDay::Unknown;
    };
    if sunrise > sunset {
        return TimeOfDay::Unknown;
    }
    if (sunrise - TWILIGHT_SECONDS..sunrise).contains(&utc_seconds) {
        TimeOfDay::Dawn
    } else if (sunrise..=sunset).contains(&utc_seconds) {
        TimeOfDay::Day
    } else if (sunset..sunset + TWILIGHT_SECONDS).contains(&utc_seconds) {
        TimeOfDay::Dusk
    } else {
        TimeOfDay::Night
    }
}

pub fn season_for(date: CivilDate, location: Location) -> Season {
    if location.latitude_degrees().abs() < 1.0 {
        return Season::Unknown;
    }
    let northern = location.latitude_degrees() > 0.0;
    let month_day = (date.month, date.day);
    let northern_season = match month_day {
        (3, 1..=31) | (4, _) | (5, _) => Season::Spring,
        (6, 1..=30) | (7, _) | (8, _) => Season::Summer,
        (9, 1..=30) | (10, _) | (11, _) => Season::Autumn,
        _ => Season::Winter,
    };
    if northern {
        northern_season
    } else {
        match northern_season {
            Season::Spring => Season::Autumn,
            Season::Summer => Season::Winter,
            Season::Autumn => Season::Spring,
            Season::Winter => Season::Summer,
            Season::Unknown => Season::Unknown,
        }
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u8) -> u8 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Howard Hinnant's proleptic Gregorian civil calendar conversion, kept here
// so snapshots do not depend on a host timezone or a non-pinned local clock.
fn days_from_civil(year: i32, month: u8, day: u8) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day = i64::from(day);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn civil_from_days(days: i64) -> (i32, u8, u8) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year as i32, month as u8, day as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct FixedAstronomy(SolarEvents);

    impl AstronomyProvider for FixedAstronomy {
        fn solar_events(
            &self,
            _date: CivilDate,
            _location: Location,
        ) -> Result<SolarEvents, EnvironmentError> {
            Ok(self.0)
        }
    }

    fn unix(date: CivilDate, hour: i64, minute: i64) -> i64 {
        date.unix_seconds_at_midnight() + hour * 3_600 + minute * 60
    }

    #[test]
    fn civil_conversion_handles_leap_day_and_epoch_boundaries() {
        let leap = CivilDate::new(2024, 2, 29).unwrap();
        let utc = unix(leap, 23, 59) + 59;
        assert_eq!(CivilDateTime::from_utc_seconds(utc, 0).unwrap().date, leap);
        assert_eq!(
            CivilDateTime::from_utc_seconds(0, 0).unwrap().date,
            CivilDate::new(1970, 1, 1).unwrap()
        );
        assert_eq!(
            CivilDateTime::from_utc_seconds(-1, 0).unwrap().date,
            CivilDate::new(1969, 12, 31).unwrap()
        );
    }

    #[test]
    fn offset_schedule_validates_order_and_models_dst_transition() {
        let schedule = UtcOffsetSchedule::new(
            -300,
            vec![OffsetTransition {
                at_utc_seconds: unix(CivilDate::new(2024, 3, 10).unwrap(), 7, 0),
                offset_minutes: -240,
            }],
        )
        .unwrap();
        let before = unix(CivilDate::new(2024, 3, 10).unwrap(), 6, 59);
        let after = unix(CivilDate::new(2024, 3, 10).unwrap(), 7, 0);
        assert_eq!(
            CivilDateTime::from_utc_seconds(before, schedule.offset_at(before))
                .unwrap()
                .hour,
            1
        );
        assert_eq!(
            CivilDateTime::from_utc_seconds(after, schedule.offset_at(after))
                .unwrap()
                .hour,
            3
        );
        assert!(
            UtcOffsetSchedule::new(
                0,
                vec![
                    OffsetTransition {
                        at_utc_seconds: 2,
                        offset_minutes: 60
                    },
                    OffsetTransition {
                        at_utc_seconds: 2,
                        offset_minutes: 0
                    }
                ]
            )
            .is_err()
        );
    }

    #[test]
    fn iana_timezone_resolves_dst_offsets_and_local_wall_time() {
        let timezone = UtcOffsetSchedule::iana("America/Los_Angeles".parse().unwrap());
        let before = unix(CivilDate::new(2024, 3, 10).unwrap(), 9, 59);
        let after = unix(CivilDate::new(2024, 3, 10).unwrap(), 10, 0);
        assert_eq!(timezone.offset_at(before), -480);
        assert_eq!(timezone.offset_at(after), -420);
        assert_eq!(timezone.local_datetime_at(before).unwrap().hour, 1);
        assert_eq!(timezone.local_datetime_at(after).unwrap().hour, 3);
        assert_eq!(
            timezone.local_datetime_at(after).unwrap().date,
            CivilDate::new(2024, 3, 10).unwrap()
        );
    }

    #[test]
    fn iana_toronto_timezone_observes_both_dst_transitions() {
        let timezone = UtcOffsetSchedule::iana("America/Toronto".parse().unwrap());
        let spring_before = unix(CivilDate::new(2024, 3, 10).unwrap(), 6, 59);
        let spring_after = unix(CivilDate::new(2024, 3, 10).unwrap(), 7, 0);
        assert_eq!(timezone.offset_at(spring_before), -300);
        assert_eq!(timezone.offset_at(spring_after), -240);
        assert_eq!(timezone.local_datetime_at(spring_before).unwrap().hour, 1);
        assert_eq!(timezone.local_datetime_at(spring_after).unwrap().hour, 3);

        let fall_before = unix(CivilDate::new(2024, 11, 3).unwrap(), 5, 59);
        let fall_after = unix(CivilDate::new(2024, 11, 3).unwrap(), 6, 0);
        assert_eq!(timezone.offset_at(fall_before), -240);
        assert_eq!(timezone.offset_at(fall_after), -300);
        assert_eq!(timezone.local_datetime_at(fall_before).unwrap().hour, 1);
        assert_eq!(timezone.local_datetime_at(fall_after).unwrap().hour, 1);
    }

    #[test]
    fn civil_date_add_days_crosses_month_and_leap_boundaries() {
        assert_eq!(
            CivilDate::new(2024, 2, 28).unwrap().add_days(1).unwrap(),
            CivilDate::new(2024, 2, 29).unwrap()
        );
        assert_eq!(
            CivilDate::new(2024, 2, 29).unwrap().add_days(1).unwrap(),
            CivilDate::new(2024, 3, 1).unwrap()
        );
        assert_eq!(
            CivilDate::new(2024, 1, 1).unwrap().add_days(-1).unwrap(),
            CivilDate::new(2023, 12, 31).unwrap()
        );
    }

    #[test]
    fn location_validation_rejects_partial_range_and_non_finite_inputs() {
        assert!(Location::new(90.0, 180.0).is_ok());
        assert!(Location::new(90.1, 0.0).is_err());
        assert!(Location::new(f64::NAN, 0.0).is_err());
    }

    #[test]
    fn classification_has_explicit_twilight_boundaries_and_polar_states() {
        let date = CivilDate::new(2024, 6, 21).unwrap();
        let sunrise = unix(date, 6, 0);
        let sunset = unix(date, 18, 0);
        assert_eq!(
            classify_time(sunrise - 30 * 60, SolarEvents::normal(sunrise, sunset)),
            TimeOfDay::Dawn
        );
        assert_eq!(
            classify_time(sunrise, SolarEvents::normal(sunrise, sunset)),
            TimeOfDay::Day
        );
        assert_eq!(
            classify_time(sunset, SolarEvents::normal(sunrise, sunset)),
            TimeOfDay::Day
        );
        assert_eq!(
            classify_time(sunset + 30 * 60, SolarEvents::normal(sunrise, sunset)),
            TimeOfDay::Night
        );
        assert_eq!(
            classify_time(0, SolarEvents::polar_day()),
            TimeOfDay::PolarDay
        );
        assert_eq!(
            classify_time(0, SolarEvents::polar_night()),
            TimeOfDay::PolarNight
        );
    }

    #[test]
    fn seasons_cover_hemispheres_and_equatorial_unknown() {
        let north = Location::new(45.0, 0.0).unwrap();
        let south = Location::new(-45.0, 0.0).unwrap();
        let equator = Location::new(0.5, 0.0).unwrap();
        assert_eq!(
            season_for(CivilDate::new(2024, 3, 1).unwrap(), north),
            Season::Spring
        );
        assert_eq!(
            season_for(CivilDate::new(2024, 3, 1).unwrap(), south),
            Season::Autumn
        );
        assert_eq!(
            season_for(CivilDate::new(2024, 2, 29).unwrap(), north),
            Season::Winter
        );
        assert_eq!(
            season_for(CivilDate::new(2024, 6, 1).unwrap(), north),
            Season::Summer
        );
        assert_eq!(
            season_for(CivilDate::new(2024, 6, 1).unwrap(), equator),
            Season::Unknown
        );
    }

    #[test]
    fn missing_inputs_are_unknown_and_snapshot_does_not_select() {
        let provider = EnvironmentProvider {
            location: None,
            timezone: None,
            astronomy: FixedAstronomy(SolarEvents::normal(1, 2)),
        };
        assert_eq!(
            provider.snapshot_at(0, Weather::Clear),
            EnvironmentSnapshot {
                time: TimeOfDay::Unknown,
                weather: Weather::Clear,
                season: Season::Unknown
            }
        );
    }

    #[test]
    fn noaa_solar_events_preserve_local_date_across_utc_midnight() {
        let date = CivilDate::new(2024, 6, 21).unwrap();
        let east = NoaaAstronomy
            .solar_events(date, Location::new(0.0, 150.0).unwrap())
            .unwrap();
        let west = NoaaAstronomy
            .solar_events(date, Location::new(0.0, -150.0).unwrap())
            .unwrap();
        let east_sunrise = east.sunrise_utc_seconds.unwrap();
        let east_sunset = east.sunset_utc_seconds.unwrap();
        let west_sunrise = west.sunrise_utc_seconds.unwrap();
        let west_sunset = west.sunset_utc_seconds.unwrap();
        assert_eq!(
            CivilDateTime::from_utc_seconds(east_sunrise, 0)
                .unwrap()
                .date,
            CivilDate::new(2024, 6, 20).unwrap()
        );
        assert_eq!(
            CivilDateTime::from_utc_seconds(east_sunset, 0)
                .unwrap()
                .date,
            date
        );
        assert_eq!(
            CivilDateTime::from_utc_seconds(west_sunrise, 0)
                .unwrap()
                .date,
            date
        );
        assert_eq!(
            CivilDateTime::from_utc_seconds(west_sunset, 0)
                .unwrap()
                .date,
            CivilDate::new(2024, 6, 22).unwrap()
        );
        let east_local = UtcOffsetSchedule::fixed(12 * 60)
            .unwrap()
            .local_datetime_at(east_sunrise)
            .unwrap();
        assert_eq!(east_local.date, date);
        assert!((6..=7).contains(&east_local.hour));
    }

    #[test]
    fn provider_uses_explicit_snapshot_inputs() {
        let date = CivilDate::new(2024, 6, 21).unwrap();
        let sunrise = unix(date, 6, 0);
        let sunset = unix(date, 18, 0);
        let provider = EnvironmentProvider {
            location: Some(Location::new(45.0, 0.0).unwrap()),
            timezone: Some(UtcOffsetSchedule::fixed(0).unwrap()),
            astronomy: FixedAstronomy(SolarEvents::normal(sunrise, sunset)),
        };
        assert_eq!(
            provider
                .snapshot_at(unix(date, 12, 0), Weather::Unknown)
                .time,
            TimeOfDay::Day
        );
        assert_eq!(
            provider
                .snapshot_at(unix(date, 12, 0), Weather::Rain)
                .weather,
            Weather::Rain
        );
    }

    #[test]
    fn noaa_provider_returns_bounded_normal_events_for_mid_latitude() {
        let events = NoaaAstronomy
            .solar_events(
                CivilDate::new(2024, 6, 21).unwrap(),
                Location::new(40.0, -74.0).unwrap(),
            )
            .unwrap();
        assert!(events.sunrise_utc_seconds.is_some());
        assert!(events.sunset_utc_seconds.is_some());
        assert!(events.sunrise_utc_seconds.unwrap() < events.sunset_utc_seconds.unwrap());
    }
}
