use std::{fmt, str::FromStr};

use chrono::{
    offset::LocalResult, DateTime, Duration, NaiveDate, NaiveDateTime, Offset, TimeZone, Utc,
};
use chrono_tz::Tz as ChronoTz;
use rrule::RRuleSet;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const STAMP_FORMAT: &str = "%Y%m%dT%H%M%S%.9f";
const DATE_FORMAT: &str = "%Y%m%d";
const MAX_GAP_SEARCH_MINUTES: i64 = 2_880;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeMode {
    AllDay,
    Floating,
    Zoned,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SeriesStart {
    AllDay { date: NaiveDate },
    Floating { local: NaiveDateTime },
    Zoned { local: NaiveDateTime, tzid: String },
}

impl SeriesStart {
    #[must_use]
    pub fn time_mode(&self) -> TimeMode {
        match self {
            Self::AllDay { .. } => TimeMode::AllDay,
            Self::Floating { .. } => TimeMode::Floating,
            Self::Zoned { .. } => TimeMode::Zoned,
        }
    }

    fn canonical_local(&self) -> NaiveDateTime {
        match self {
            Self::AllDay { date } => date
                .and_hms_opt(0, 0, 0)
                .expect("midnight is always a valid naive time"),
            Self::Floating { local } | Self::Zoned { local, .. } => *local,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum OccurrenceKey {
    AllDay {
        original_date: NaiveDate,
    },
    Floating {
        original_local: NaiveDateTime,
    },
    Zoned {
        original_local: NaiveDateTime,
        tzid: String,
    },
}

impl OccurrenceKey {
    fn from_start(start: &SeriesStart, original_local: NaiveDateTime) -> Self {
        match start {
            SeriesStart::AllDay { .. } => Self::AllDay {
                original_date: original_local.date(),
            },
            SeriesStart::Floating { .. } => Self::Floating { original_local },
            SeriesStart::Zoned { tzid, .. } => Self::Zoned {
                original_local,
                tzid: tzid.clone(),
            },
        }
    }
}

impl fmt::Display for OccurrenceKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AllDay { original_date } => {
                write!(formatter, "v1|d|{}", original_date.format(DATE_FORMAT))
            }
            Self::Floating { original_local } => {
                write!(formatter, "v1|f|{}", original_local.format(STAMP_FORMAT))
            }
            Self::Zoned {
                original_local,
                tzid,
            } => write!(
                formatter,
                "v1|z|{tzid}|{}",
                original_local.format(STAMP_FORMAT)
            ),
        }
    }
}

impl FromStr for OccurrenceKey {
    type Err = RecurrenceError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parts = value.split('|').collect::<Vec<_>>();
        match parts.as_slice() {
            ["v1", "d", date] => Ok(Self::AllDay {
                original_date: NaiveDate::parse_from_str(date, DATE_FORMAT)
                    .map_err(|_| RecurrenceError::InvalidOccurrenceKey(value.to_owned()))?,
            }),
            ["v1", "f", local] => Ok(Self::Floating {
                original_local: parse_stamp(local, value)?,
            }),
            ["v1", "z", tzid, local] if !tzid.is_empty() => {
                tzid.parse::<ChronoTz>()
                    .map_err(|_| RecurrenceError::InvalidTimezone((*tzid).to_owned()))?;
                Ok(Self::Zoned {
                    original_local: parse_stamp(local, value)?,
                    tzid: (*tzid).to_owned(),
                })
            }
            _ => Err(RecurrenceError::InvalidOccurrenceKey(value.to_owned())),
        }
    }
}

fn parse_stamp(value: &str, key: &str) -> Result<NaiveDateTime, RecurrenceError> {
    NaiveDateTime::parse_from_str(value, STAMP_FORMAT)
        .map_err(|_| RecurrenceError::InvalidOccurrenceKey(key.to_owned()))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OccurrenceRef {
    pub series_id: Uuid,
    pub occurrence_key: OccurrenceKey,
}

impl fmt::Display for OccurrenceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.series_id, self.occurrence_key)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecurrenceDefinition {
    pub series_id: Uuid,
    pub start: SeriesStart,
    /// RFC 5545 RRULE value without the `RRULE:` prefix.
    pub rrule: String,
    /// Additional original local date-times. All-day values must be midnight.
    #[serde(default)]
    pub rdates: Vec<NaiveDateTime>,
    /// Excluded original local date-times. All-day values must be midnight.
    #[serde(default)]
    pub exdates: Vec<NaiveDateTime>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OccurrenceProjection {
    pub occurrence_ref: OccurrenceRef,
    pub time_mode: TimeMode,
    pub original_local: NaiveDateTime,
    pub scheduled_local: NaiveDateTime,
    pub scheduled_utc: Option<DateTime<Utc>>,
    pub chosen_offset_seconds: Option<i32>,
    pub dst_adjusted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZonedResolution {
    pub scheduled_local: NaiveDateTime,
    pub scheduled_utc: DateTime<Utc>,
    pub chosen_offset_seconds: i32,
    pub dst_adjusted: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RecurrenceError {
    #[error("invalid occurrence key: {0}")]
    InvalidOccurrenceKey(String),
    #[error("invalid IANA timezone: {0}")]
    InvalidTimezone(String),
    #[error("RRULE must be a single rule value without DTSTART, RDATE, or EXDATE")]
    InvalidRuleShape,
    #[error("invalid recurrence rule: {0}")]
    InvalidRule(String),
    #[error("all-day RDATE and EXDATE values must be midnight")]
    NonMidnightAllDayDate,
    #[error("could not resolve local time {local} in timezone {tzid}")]
    UnresolvableLocalTime { local: NaiveDateTime, tzid: String },
}

/// Expands occurrences without mutating persistent state. The RFC engine runs on a
/// UTC-shaped copy of each original wall-clock value; timezone resolution is then
/// applied explicitly so DST policy never depends on an upstream library default.
pub fn expand_virtual(
    definition: &RecurrenceDefinition,
    range_start_local: NaiveDateTime,
    range_end_local: NaiveDateTime,
    limit: u16,
) -> Result<Vec<OccurrenceProjection>, RecurrenceError> {
    if range_end_local <= range_start_local || limit == 0 {
        return Ok(Vec::new());
    }
    validate_definition(definition)?;

    let rule_value = definition.rrule.trim();
    let mut content = format!(
        "DTSTART:{}Z\nRRULE:{}",
        definition.start.canonical_local().format("%Y%m%dT%H%M%S"),
        rule_value
    );
    append_dates(&mut content, "RDATE", &definition.rdates);
    append_dates(&mut content, "EXDATE", &definition.exdates);

    let rule_set = content
        .parse::<RRuleSet>()
        .map_err(|error| RecurrenceError::InvalidRule(error.to_string()))?;

    let generated = rule_set.all(limit);
    generated
        .dates
        .into_iter()
        .map(|value| value.naive_utc())
        .filter(|local| *local >= range_start_local && *local < range_end_local)
        .map(|original_local| project(definition, original_local))
        .collect()
}

/// Reconstructs one projection from its stable key and verifies that the key
/// matches the series time semantics. Membership in the RRULE is validated by
/// callers before a state-changing operation.
pub fn project_from_key(
    definition: &RecurrenceDefinition,
    occurrence_key: &OccurrenceKey,
) -> Result<OccurrenceProjection, RecurrenceError> {
    let original_local = match (&definition.start, occurrence_key) {
        (SeriesStart::AllDay { .. }, OccurrenceKey::AllDay { original_date }) => original_date
            .and_hms_opt(0, 0, 0)
            .expect("midnight is always valid"),
        (SeriesStart::Floating { .. }, OccurrenceKey::Floating { original_local }) => {
            *original_local
        }
        (
            SeriesStart::Zoned { tzid, .. },
            OccurrenceKey::Zoned {
                original_local,
                tzid: key_tzid,
            },
        ) if tzid == key_tzid => *original_local,
        _ => {
            return Err(RecurrenceError::InvalidOccurrenceKey(
                occurrence_key.to_string(),
            ));
        }
    };
    project(definition, original_local)
}

fn validate_definition(definition: &RecurrenceDefinition) -> Result<(), RecurrenceError> {
    let rule = definition.rrule.trim();
    if rule.is_empty()
        || rule.contains(['\r', '\n'])
        || rule.to_ascii_uppercase().starts_with("RRULE:")
        || rule.to_ascii_uppercase().contains("DTSTART")
        || rule.to_ascii_uppercase().contains("RDATE")
        || rule.to_ascii_uppercase().contains("EXDATE")
    {
        return Err(RecurrenceError::InvalidRuleShape);
    }

    if matches!(definition.start, SeriesStart::AllDay { .. })
        && definition
            .rdates
            .iter()
            .chain(&definition.exdates)
            .any(|value| value.time() != chrono::NaiveTime::MIN)
    {
        return Err(RecurrenceError::NonMidnightAllDayDate);
    }

    if let SeriesStart::Zoned { tzid, .. } = &definition.start {
        if tzid.contains('|') {
            return Err(RecurrenceError::InvalidTimezone(tzid.clone()));
        }
        tzid.parse::<ChronoTz>()
            .map_err(|_| RecurrenceError::InvalidTimezone(tzid.clone()))?;
    }
    Ok(())
}

fn append_dates(content: &mut String, property: &str, values: &[NaiveDateTime]) {
    if values.is_empty() {
        return;
    }
    let values = values
        .iter()
        .map(|value| format!("{}Z", value.format("%Y%m%dT%H%M%S")))
        .collect::<Vec<_>>()
        .join(",");
    content.push('\n');
    content.push_str(property);
    content.push(':');
    content.push_str(&values);
}

fn project(
    definition: &RecurrenceDefinition,
    original_local: NaiveDateTime,
) -> Result<OccurrenceProjection, RecurrenceError> {
    let key = OccurrenceKey::from_start(&definition.start, original_local);
    let occurrence_ref = OccurrenceRef {
        series_id: definition.series_id,
        occurrence_key: key,
    };

    match &definition.start {
        SeriesStart::AllDay { .. } => Ok(OccurrenceProjection {
            occurrence_ref,
            time_mode: TimeMode::AllDay,
            original_local,
            scheduled_local: original_local,
            scheduled_utc: None,
            chosen_offset_seconds: None,
            dst_adjusted: false,
        }),
        SeriesStart::Floating { .. } => Ok(OccurrenceProjection {
            occurrence_ref,
            time_mode: TimeMode::Floating,
            original_local,
            scheduled_local: original_local,
            scheduled_utc: None,
            chosen_offset_seconds: None,
            dst_adjusted: false,
        }),
        SeriesStart::Zoned { tzid, .. } => {
            let resolved = resolve_zoned_local(original_local, tzid)?;
            Ok(OccurrenceProjection {
                occurrence_ref,
                time_mode: TimeMode::Zoned,
                original_local,
                scheduled_local: resolved.scheduled_local,
                scheduled_utc: Some(resolved.scheduled_utc),
                chosen_offset_seconds: Some(resolved.chosen_offset_seconds),
                dst_adjusted: resolved.dst_adjusted,
            })
        }
    }
}

/// Resolves a wall-clock time using the frozen product policy:
/// nonexistent local times shift forward by the gap; ambiguous local times use
/// the instant with the earlier UTC timestamp.
pub fn resolve_zoned_local(
    local: NaiveDateTime,
    tzid: &str,
) -> Result<ZonedResolution, RecurrenceError> {
    let timezone = tzid
        .parse::<ChronoTz>()
        .map_err(|_| RecurrenceError::InvalidTimezone(tzid.to_owned()))?;

    match timezone.from_local_datetime(&local) {
        LocalResult::Single(value) => Ok(resolution(local, value, false)),
        LocalResult::Ambiguous(first, second) => {
            let selected = if first.with_timezone(&Utc) <= second.with_timezone(&Utc) {
                first
            } else {
                second
            };
            Ok(resolution(local, selected, false))
        }
        LocalResult::None => resolve_gap(local, tzid, timezone),
    }
}

fn resolve_gap(
    local: NaiveDateTime,
    tzid: &str,
    timezone: ChronoTz,
) -> Result<ZonedResolution, RecurrenceError> {
    let before = (1..=MAX_GAP_SEARCH_MINUTES).find_map(|minutes| {
        select_earlier(timezone.from_local_datetime(&(local - Duration::minutes(minutes))))
    });
    let after = (1..=MAX_GAP_SEARCH_MINUTES).find_map(|minutes| {
        select_earlier(timezone.from_local_datetime(&(local + Duration::minutes(minutes))))
    });

    let (before, after) =
        before
            .zip(after)
            .ok_or_else(|| RecurrenceError::UnresolvableLocalTime {
                local,
                tzid: tzid.to_owned(),
            })?;
    let gap_seconds =
        after.offset().fix().local_minus_utc() - before.offset().fix().local_minus_utc();
    if gap_seconds <= 0 {
        return Err(RecurrenceError::UnresolvableLocalTime {
            local,
            tzid: tzid.to_owned(),
        });
    }

    let shifted = local + Duration::seconds(i64::from(gap_seconds));
    let selected = select_earlier(timezone.from_local_datetime(&shifted)).ok_or_else(|| {
        RecurrenceError::UnresolvableLocalTime {
            local,
            tzid: tzid.to_owned(),
        }
    })?;
    Ok(resolution(shifted, selected, true))
}

fn select_earlier(result: LocalResult<DateTime<ChronoTz>>) -> Option<DateTime<ChronoTz>> {
    match result {
        LocalResult::Single(value) => Some(value),
        LocalResult::Ambiguous(first, second) => {
            if first.with_timezone(&Utc) <= second.with_timezone(&Utc) {
                Some(first)
            } else {
                Some(second)
            }
        }
        LocalResult::None => None,
    }
}

fn resolution(
    scheduled_local: NaiveDateTime,
    value: DateTime<ChronoTz>,
    dst_adjusted: bool,
) -> ZonedResolution {
    ZonedResolution {
        scheduled_local,
        scheduled_utc: value.with_timezone(&Utc),
        chosen_offset_seconds: value.offset().fix().local_minus_utc(),
        dst_adjusted,
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Datelike, Timelike};

    use super::*;

    fn local(value: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").expect("valid test date")
    }

    #[test]
    fn occurrence_keys_round_trip_and_preserve_original_identity() {
        let keys = [
            OccurrenceKey::AllDay {
                original_date: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
            },
            OccurrenceKey::Floating {
                original_local: local("2026-09-03 09:15:00"),
            },
            OccurrenceKey::Zoned {
                original_local: local("2026-09-03 09:15:00"),
                tzid: "Asia/Shanghai".to_owned(),
            },
        ];
        for key in keys {
            let encoded = key.to_string();
            assert_eq!(encoded.parse::<OccurrenceKey>().unwrap(), key);
        }
    }

    #[test]
    fn daily_weekday_and_multi_week_rules_are_virtual() {
        let weekday = RecurrenceDefinition {
            series_id: Uuid::now_v7(),
            start: SeriesStart::Floating {
                local: local("2026-09-01 09:00:00"),
            },
            rrule: "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;COUNT=10".to_owned(),
            rdates: Vec::new(),
            exdates: Vec::new(),
        };
        let dates = expand_virtual(
            &weekday,
            local("2026-09-01 00:00:00"),
            local("2026-10-01 00:00:00"),
            100,
        )
        .unwrap();
        assert_eq!(dates.len(), 10);
        assert!(dates
            .iter()
            .all(|date| date.original_local.weekday().number_from_monday() <= 5));

        let mut fortnightly = weekday;
        fortnightly.rrule = "FREQ=WEEKLY;INTERVAL=2;COUNT=4".to_owned();
        let dates = expand_virtual(
            &fortnightly,
            local("2026-09-01 00:00:00"),
            local("2026-11-01 00:00:00"),
            100,
        )
        .unwrap();
        assert_eq!(dates.len(), 4);
        assert_eq!(
            (dates[1].original_local - dates[0].original_local).num_days(),
            14
        );
    }

    #[test]
    fn monthly_end_of_month_and_leap_day_follow_rfc_calendar_rules() {
        let end_of_month = RecurrenceDefinition {
            series_id: Uuid::now_v7(),
            start: SeriesStart::AllDay {
                date: NaiveDate::from_ymd_opt(2024, 1, 31).unwrap(),
            },
            rrule: "FREQ=MONTHLY;BYMONTHDAY=-1;COUNT=3".to_owned(),
            rdates: Vec::new(),
            exdates: Vec::new(),
        };
        let dates = expand_virtual(
            &end_of_month,
            local("2024-01-01 00:00:00"),
            local("2024-05-01 00:00:00"),
            10,
        )
        .unwrap();
        assert_eq!(dates[0].original_local.day(), 31);
        assert_eq!(
            dates[1].original_local.date(),
            NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()
        );
        assert_eq!(dates[2].original_local.day(), 31);

        let leap = RecurrenceDefinition {
            series_id: Uuid::now_v7(),
            start: SeriesStart::AllDay {
                date: NaiveDate::from_ymd_opt(2024, 2, 29).unwrap(),
            },
            rrule: "FREQ=YEARLY;COUNT=2".to_owned(),
            rdates: Vec::new(),
            exdates: Vec::new(),
        };
        let dates = expand_virtual(
            &leap,
            local("2024-01-01 00:00:00"),
            local("2030-01-01 00:00:00"),
            10,
        )
        .unwrap();
        assert_eq!(
            dates[1].original_local.date(),
            NaiveDate::from_ymd_opt(2028, 2, 29).unwrap()
        );
    }

    #[test]
    fn rdate_exdate_count_and_until_are_honored() {
        let definition = RecurrenceDefinition {
            series_id: Uuid::now_v7(),
            start: SeriesStart::Floating {
                local: local("2026-09-01 09:00:00"),
            },
            rrule: "FREQ=DAILY;COUNT=3".to_owned(),
            rdates: vec![local("2026-09-10 09:00:00")],
            exdates: vec![local("2026-09-02 09:00:00")],
        };
        let dates = expand_virtual(
            &definition,
            local("2026-09-01 00:00:00"),
            local("2026-10-01 00:00:00"),
            20,
        )
        .unwrap();
        assert_eq!(dates.len(), 3);
        assert!(dates.iter().any(|date| date.original_local.day() == 10));
        assert!(!dates.iter().any(|date| date.original_local.day() == 2));

        let mut until = definition;
        until.rdates.clear();
        until.exdates.clear();
        until.rrule = "FREQ=DAILY;UNTIL=20260903T090000Z".to_owned();
        let dates = expand_virtual(
            &until,
            local("2026-09-01 00:00:00"),
            local("2026-10-01 00:00:00"),
            20,
        )
        .unwrap();
        assert_eq!(dates.len(), 3);
    }

    #[test]
    fn dst_gap_shifts_forward_by_the_gap_but_key_keeps_original_time() {
        let definition = RecurrenceDefinition {
            series_id: Uuid::now_v7(),
            start: SeriesStart::Zoned {
                local: local("2026-03-01 02:30:00"),
                tzid: "America/New_York".to_owned(),
            },
            rrule: "FREQ=WEEKLY;COUNT=3".to_owned(),
            rdates: Vec::new(),
            exdates: Vec::new(),
        };
        let dates = expand_virtual(
            &definition,
            local("2026-03-01 00:00:00"),
            local("2026-04-01 00:00:00"),
            20,
        )
        .unwrap();
        let gap = &dates[1];
        assert_eq!(gap.original_local, local("2026-03-08 02:30:00"));
        assert_eq!(gap.scheduled_local, local("2026-03-08 03:30:00"));
        assert!(gap.dst_adjusted);
        assert!(gap
            .occurrence_ref
            .occurrence_key
            .to_string()
            .ends_with("20260308T023000.000000000"));
    }

    #[test]
    fn dst_fold_chooses_the_earlier_utc_instant_once() {
        let resolved =
            resolve_zoned_local(local("2026-11-01 01:30:00"), "America/New_York").unwrap();
        assert_eq!(resolved.scheduled_utc.hour(), 5);
        assert_eq!(resolved.chosen_offset_seconds, -4 * 60 * 60);
        assert!(!resolved.dst_adjusted);
    }
}
