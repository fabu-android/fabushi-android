#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SandUpdateTrack {
    Stable,
    Nightly,
    Dogfood,
}

impl SandUpdateTrack {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "stable" => Some(Self::Stable),
            "nightly" => Some(Self::Nightly),
            "dogfood" => Some(Self::Dogfood),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Nightly => "nightly",
            Self::Dogfood => "dogfood",
        }
    }
}

pub const SAND_UPDATE_TRACKS: &[&str] = &["stable", "nightly", "dogfood"];
pub const NIGHTLY_UPDATE_TRACK_DISABLED: bool = true;

pub fn is_sand_update_track(value: &str) -> bool {
    SandUpdateTrack::parse(value).is_some()
}

pub fn coerce_to_enabled_track(track: SandUpdateTrack) -> SandUpdateTrack {
    if NIGHTLY_UPDATE_TRACK_DISABLED && track == SandUpdateTrack::Nightly {
        SandUpdateTrack::Stable
    } else {
        track
    }
}

pub fn selectable_update_tracks(unlock_internal_tracks: bool) -> Vec<SandUpdateTrack> {
    [SandUpdateTrack::Stable, SandUpdateTrack::Nightly, SandUpdateTrack::Dogfood]
        .into_iter()
        .filter(|track| {
            !(*track == SandUpdateTrack::Nightly && NIGHTLY_UPDATE_TRACK_DISABLED)
                && (*track != SandUpdateTrack::Dogfood || unlock_internal_tracks)
        })
        .collect()
}

pub fn available_update_tracks(
    unlock_internal_tracks: bool,
    effective_track: SandUpdateTrack,
) -> Vec<SandUpdateTrack> {
    let selectable = selectable_update_tracks(unlock_internal_tracks);
    [SandUpdateTrack::Stable, SandUpdateTrack::Nightly, SandUpdateTrack::Dogfood]
        .into_iter()
        .filter(|track| selectable.contains(track) || *track == effective_track)
        .collect()
}

pub fn to_managed_update_track(value: &str) -> Option<SandUpdateTrack> {
    let track = SandUpdateTrack::parse(value.trim())?;
    (coerce_to_enabled_track(track) == track).then_some(track)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseTrackGate {
    pub managed_track: Option<SandUpdateTrack>,
    pub unlock_internal_tracks: bool,
}

pub fn resolve_release_track_gate(
    release_track: Option<&str>,
    unlock_internal_tracks: bool,
) -> ReleaseTrackGate {
    ReleaseTrackGate {
        managed_track: release_track.and_then(to_managed_update_track),
        unlock_internal_tracks,
    }
}

pub fn resolve_effective_track(
    managed_track: Option<SandUpdateTrack>,
    user_override: Option<SandUpdateTrack>,
    build_default: Option<SandUpdateTrack>,
) -> SandUpdateTrack {
    coerce_to_enabled_track(
        managed_track
            .or(user_override)
            .or(build_default)
            .unwrap_or(SandUpdateTrack::Stable),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_known_update_tracks_are_accepted() {
        assert!(is_sand_update_track("stable"));
        assert!(is_sand_update_track("nightly"));
        assert!(is_sand_update_track("dogfood"));
        assert!(!is_sand_update_track("beta"));
    }

    #[test]
    fn selectable_tracks_match_desktop_release_gate() {
        assert_eq!(selectable_update_tracks(false), vec![SandUpdateTrack::Stable]);
        assert_eq!(
            selectable_update_tracks(true),
            vec![SandUpdateTrack::Stable, SandUpdateTrack::Dogfood],
        );
        assert_eq!(
            available_update_tracks(false, SandUpdateTrack::Dogfood),
            vec![SandUpdateTrack::Stable, SandUpdateTrack::Dogfood],
        );
        assert_eq!(
            available_update_tracks(false, SandUpdateTrack::Nightly),
            vec![SandUpdateTrack::Stable, SandUpdateTrack::Nightly],
        );
    }

    #[test]
    fn managed_and_effective_tracks_preserve_desktop_precedence_and_disable_nightly() {
        assert_eq!(to_managed_update_track(" stable "), Some(SandUpdateTrack::Stable));
        assert_eq!(to_managed_update_track("dogfood"), Some(SandUpdateTrack::Dogfood));
        assert_eq!(to_managed_update_track("nightly"), None);
        assert_eq!(to_managed_update_track("beta"), None);

        let gate = resolve_release_track_gate(Some("dogfood"), true);
        assert_eq!(gate.managed_track, Some(SandUpdateTrack::Dogfood));
        assert!(gate.unlock_internal_tracks);

        assert_eq!(
            resolve_effective_track(
                Some(SandUpdateTrack::Dogfood),
                Some(SandUpdateTrack::Stable),
                Some(SandUpdateTrack::Stable),
            ),
            SandUpdateTrack::Dogfood,
        );
        assert_eq!(
            resolve_effective_track(
                None,
                Some(SandUpdateTrack::Nightly),
                Some(SandUpdateTrack::Dogfood),
            ),
            SandUpdateTrack::Stable,
        );
        assert_eq!(
            resolve_effective_track(None, None, None),
            SandUpdateTrack::Stable,
        );
    }
}
