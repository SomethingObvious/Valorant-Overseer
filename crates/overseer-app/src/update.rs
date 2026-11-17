use overseer_core::Bridge;

use crate::settings::UpdateSettings;

const DAY: i64 = 24 * 60 * 60;

/// This build's version, which a newer release is measured against.
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

pub(crate) const LATEST: &str =
    "https://github.com/SomethingObvious/Valorant-Overseer/releases/latest";

#[derive(Debug, Default)]
pub(crate) struct Update {
    waiting: Option<u64>,
}

impl Update {
    pub(crate) fn waiting_on(&self, id: u64) -> bool {
        self.waiting == Some(id)
    }

    pub(crate) const fn reconnected(&mut self) {
        self.waiting = None;
    }

    pub(crate) fn ask(&mut self, bridge: &Bridge, settings: &UpdateSettings, now: i64) {
        if due(settings, now) && self.waiting.is_none() {
            self.waiting = Some(bridge.ask("newest", serde_json::Value::Null));
        }
    }

    /// Keeps what came back. A failed answer still counts as asked, so a PC
    /// that can't reach GitHub asks once a day and no more.
    pub(crate) fn answered(
        &mut self,
        result: &Result<serde_json::Value, String>,
        settings: &mut UpdateSettings,
        now: i64,
    ) {
        self.waiting = None;
        settings.update_checked_at = Some(now);
        if let Some(newest) = result
            .as_ref()
            .ok()
            .and_then(|v| v.get("newest"))
            .and_then(|v| v.as_str())
            .filter(|v| parse(v).is_some())
        {
            settings.newest_release = Some(newest.to_owned());
        }
    }
}

fn due(settings: &UpdateSettings, now: i64) -> bool {
    settings.check_updates
        && settings
            .update_checked_at
            .is_none_or(|at| now - at >= DAY || at > now)
}

pub(crate) fn newer<'a>(settings: &'a UpdateSettings, current: &str) -> Option<&'a str> {
    let newest = settings.newest_release.as_deref()?;
    (settings.check_updates && parse(newest)? > parse(current)?).then_some(newest)
}

pub(crate) fn page(version: &str) -> Option<String> {
    let (major, minor, patch) = parse(version)?;
    Some(format!(
        "https://github.com/SomethingObvious/Valorant-Overseer/releases/tag/v{major}.{minor}.{patch}"
    ))
}

fn parse(version: &str) -> Option<(u32, u32, u32)> {
    let mut parts = version.split('.').map(|part| {
        (!part.is_empty() && part.len() <= 4 && part.bytes().all(|b| b.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    });
    let found = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(found)
}

/// Seconds since 1970, by the PC's clock.
pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::{DAY, Update, UpdateSettings, due, newer, page};

    #[test]
    fn only_a_newer_release_is_said() {
        let at = |newest: &str| UpdateSettings {
            newest_release: Some(newest.to_owned()),
            ..UpdateSettings::default()
        };
        assert_eq!(newer(&at("2.2.0"), "2.1.0"), Some("2.2.0"));
        assert_eq!(newer(&at("2.10.0"), "2.9.3"), Some("2.10.0"));
        assert_eq!(newer(&at("2.1.0"), "2.1.0"), None);
        assert_eq!(newer(&at("2.0.9"), "2.1.0"), None);
        for odd in [
            "2.2",
            "2.2.0.1",
            "v2.2.0",
            "2.2.0-beta",
            "",
            "2..0",
            "99999.0.0",
        ] {
            assert_eq!(newer(&at(odd), "2.1.0"), None, "{odd}");
        }
        let off = UpdateSettings {
            check_updates: false,
            ..at("9.0.0")
        };
        assert_eq!(newer(&off, "2.1.0"), None);
        assert_eq!(
            page("2.2.0").as_deref(),
            Some("https://github.com/SomethingObvious/Valorant-Overseer/releases/tag/v2.2.0")
        );
        assert_eq!(page("2.2.0/../x"), None);
    }

    /// It asks once a day at most, not at all when switched off, again when
    /// the clock went backwards, and a failed answer still waits a day.
    #[test]
    fn it_asks_once_a_day_at_most() {
        let mut settings = UpdateSettings::default();
        assert!(due(&settings, 1_000_000));
        let mut update = Update::default();
        update.answered(&Err("offline".to_owned()), &mut settings, 1_000_000);
        assert_eq!(settings.newest_release, None);
        assert!(!due(&settings, 1_000_000 + DAY - 1));
        assert!(due(&settings, 1_000_000 + DAY));
        assert!(due(&settings, 999_000), "a clock set back");
        update.answered(
            &Ok(serde_json::json!({ "newest": "2.2.0" })),
            &mut settings,
            1_000_000 + DAY,
        );
        assert_eq!(settings.newest_release.as_deref(), Some("2.2.0"));
        update.answered(
            &Ok(serde_json::json!({ "newest": "not a version" })),
            &mut settings,
            1_000_000 + 2 * DAY,
        );
        assert_eq!(settings.newest_release.as_deref(), Some("2.2.0"), "kept");
        settings.check_updates = false;
        assert!(!due(&settings, 1_000_000 + 9 * DAY));
    }
}
