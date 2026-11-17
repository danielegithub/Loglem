use anyhow::{bail, Result};
use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use serde::{
    de::{self, IgnoredAny, MapAccess, SeqAccess, Visitor},
    Deserialize, Deserializer,
};
use std::{borrow::Cow, fmt};

/// Only timestamp fields are materialized. Unknown log payloads are skipped by
/// serde instead of allocating a complete serde_json::Value tree for every line.
#[derive(Default)]
struct Stamp<'a> {
    text: Option<Cow<'a, str>>,
    seconds: Option<f64>,
}
impl<'de: 'a, 'a> Deserialize<'de> for Stamp<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StampVisitor<'a>(std::marker::PhantomData<&'a ()>);
        impl<'de: 'a, 'a> Visitor<'de> for StampVisitor<'a> {
            type Value = Stamp<'a>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a timestamp or an ignored JSON value")
            }
            fn visit_borrowed_str<E: de::Error>(self, value: &'de str) -> Result<Self::Value, E> {
                Ok(Stamp {
                    text: Some(Cow::Borrowed(value)),
                    seconds: None,
                })
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(Stamp {
                    text: Some(Cow::Owned(value.into())),
                    seconds: None,
                })
            }
            fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(Stamp {
                    text: Some(Cow::Owned(value)),
                    seconds: None,
                })
            }
            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(Stamp {
                    text: None,
                    seconds: Some(value),
                })
            }
            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                self.visit_f64(value as f64)
            }
            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                self.visit_f64(value as f64)
            }
            fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(Stamp::default())
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(Stamp::default())
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                while seq.next_element::<IgnoredAny>()?.is_some() {}
                Ok(Stamp::default())
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
                Ok(Stamp::default())
            }
        }
        deserializer.deserialize_any(StampVisitor(std::marker::PhantomData))
    }
}

#[derive(Deserialize)]
struct LogDates<'a> {
    #[serde(borrow, rename = "StartUTC")]
    start: Option<Stamp<'a>>,
    #[serde(borrow)]
    ts: Option<Stamp<'a>>,
    #[serde(borrow)]
    timestamp: Option<Stamp<'a>>,
    #[serde(borrow)]
    time: Option<Stamp<'a>>,
    #[serde(borrow, rename = "@timestamp")]
    at_timestamp: Option<Stamp<'a>>,
}

pub fn now_ns() -> i64 {
    Utc::now()
        .timestamp_nanos_opt()
        .expect("current date fits nanoseconds")
}
pub fn format_ns(value: i64) -> String {
    DateTime::<Utc>::from_timestamp_nanos(value).to_rfc3339_opts(SecondsFormat::Nanos, true)
}

pub fn parse_date(text: &str, end: bool) -> Result<i64> {
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        let date = if end {
            date.succ_opt()
                .ok_or_else(|| anyhow::anyhow!("Date out of range"))?
        } else {
            date
        };
        return date
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp_nanos_opt()
            .ok_or_else(|| anyhow::anyhow!("Date out of range"));
    }
    if let Ok(date) = DateTime::parse_from_rfc3339(text) {
        if let Some(ns) = date.timestamp_nanos_opt() {
            return Ok(ns);
        }
    }
    bail!("Use YYYY-MM-DD or an RFC3339 timestamp with timezone, for example 2026-11-16T14:00:00Z")
}

/// Preserve unknown formats verbatim. Never call collection time an event time.
pub fn event_time(text: &str, collected: i64) -> (i64, &'static str) {
    if let Some(dates) = text
        .trim_start()
        .starts_with('{')
        .then(|| serde_json::from_str::<LogDates<'_>>(text).ok())
        .flatten()
    {
        for (value, numeric) in [
            (dates.ts, true),
            (dates.timestamp, false),
            (dates.start, false),
            (dates.time, false),
            (dates.at_timestamp, false),
        ] {
            if let Some(value) = value {
                if let Some(s) = value.text.as_deref() {
                    if let Ok(t) = parse_date(s, false) {
                        return (t, "event");
                    }
                }
                // Caddy's numeric ts is Unix seconds, including its fractional part.
                if numeric {
                    if let Some(seconds) = value.seconds {
                        let ns = seconds * 1_000_000_000.0;
                        if ns.is_finite() && ns >= i64::MIN as f64 && ns < i64::MAX as f64 {
                            return (ns as i64, "event");
                        }
                    }
                }
            }
        }
    }
    if let Some(first) = text.split_whitespace().next() {
        if let Ok(t) = DateTime::parse_from_rfc3339(first.trim_matches(['[', ']'])) {
            if let Some(ns) = t.timestamp_nanos_opt() {
                return (ns, "event");
            }
        }
    }
    (collected, "collection")
}

pub fn cutoff(now: i64, days: u32) -> i64 {
    now.saturating_sub(i64::from(days).saturating_mul(86_400_000_000_000))
}
