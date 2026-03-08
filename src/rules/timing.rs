
use crate::context::ValidationContext;
use crate::diagnostic::{BoxPath, DiagnosticType, PropertyPath};
use crate::rules::ValidationRule;
use isobmff_syntax::boxes::{
    EditListBox, EditListBoxView, MediaHeaderBox, MediaHeaderBoxView, MovieHeaderBox,
    MovieHeaderBoxView,
};
use isobmff_syntax::{BoxCode, FullBoxHeader, RawBox};

pub struct TimescaleNonzeroRule;

impl ValidationRule for TimescaleNonzeroRule {
    fn code(&self) -> &'static str {
        "E030"
    }

    fn name(&self) -> &'static str {
        "timescale-nonzero"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        match raw_box.box_type() {
            BoxCode::MVHD => self.validate_mvhd(ctx, raw_box),
            BoxCode::MDHD => self.validate_mdhd(ctx, raw_box),
            _ => {}
        }
    }
}

impl TimescaleNonzeroRule {
    fn validate_mvhd(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(mvhd) = MovieHeaderBoxView::new(raw_box.data()) else {
            ctx.emit(DiagnosticType::MvhdParseFailed);
            return;
        };

        if mvhd.timescale() == 0 {
            ctx.emit_property(DiagnosticType::MvhdTimescaleZero, PropertyPath::from("timescale"));
        }
    }

    fn validate_mdhd(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(mdhd) = MediaHeaderBoxView::new(raw_box.data()) else {
            ctx.emit(DiagnosticType::MdhdParseFailed);
            return;
        };

        if mdhd.timescale() == 0 {
            ctx.emit_property(DiagnosticType::MdhdTimescaleZero, PropertyPath::from("timescale"));
        }
    }
}

pub struct MvhdNextTrackIdNonzeroRule;

impl ValidationRule for MvhdNextTrackIdNonzeroRule {
    fn code(&self) -> &'static str {
        "M003"
    }

    fn name(&self) -> &'static str {
        "mvhd-next-track-id-nonzero"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::MVHD {
            return;
        }

        let Ok(mvhd) = MovieHeaderBoxView::new(raw_box.data()) else {
            return;
        };

        if mvhd.next_track_id() == 0 {
            ctx.emit_property(DiagnosticType::MvhdNextTrackIdZero, PropertyPath::from("next_track_id"));
        }
    }
}

pub struct MvhdNextTrackIdGreaterRule;

impl ValidationRule for MvhdNextTrackIdGreaterRule {
    fn code(&self) -> &'static str {
        "M004"
    }

    fn name(&self) -> &'static str {
        "mvhd-next-track-id-greater"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let Some(next_id) = state.next_track_id else {
            return;
        };

        if next_id == 0xFFFFFFFF {
            return;
        }

        let mvhd_path = state.mvhd_path.clone();
        if let Some(&max_id) = state.seen_track_ids.iter().max()
            && next_id <= max_id {
                ctx.emit_at_property(DiagnosticType::MvhdNextTrackIdTooSmall { next_id, max_id }, mvhd_path, PropertyPath::from("next_track_id"));
            }
    }
}

pub struct MvhdVersionRule;

impl ValidationRule for MvhdVersionRule {
    fn code(&self) -> &'static str {
        "M005"
    }

    fn name(&self) -> &'static str {
        "mvhd-version"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::MVHD {
            return;
        }

        let Ok(header) = FullBoxHeader::parse(raw_box.data(), raw_box.data().len()) else {
            return;
        };
        if header.version > 1 {
            ctx.emit_property(DiagnosticType::MvhdInvalidVersion { version: header.version }, PropertyPath::from("version"));
        }
    }
}

pub struct MvhdDurationConsistencyRule;

impl ValidationRule for MvhdDurationConsistencyRule {
    fn code(&self) -> &'static str {
        "M006"
    }

    fn name(&self) -> &'static str {
        "mvhd-duration-consistency"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let Some(mvhd_duration) = state.mvhd_duration else {
            return;
        };
        let Some(mvhd_timescale) = state.mvhd_timescale else {
            return;
        };
        if mvhd_timescale == 0 || mvhd_duration == u64::MAX {
            return;
        }

        let max_tkhd_duration = state.track_info.values()
            .map(|info| info.tkhd_duration)
            .max();

        let mvhd_path = state.mvhd_path.clone();
        if let Some(max_tkhd) = max_tkhd_duration
            && max_tkhd != u64::MAX {
                let diff = mvhd_duration.abs_diff(max_tkhd);
                if diff > 1 {
                    ctx.emit_at_property(DiagnosticType::MvhdDurationMismatch { mvhd_duration, max_track: max_tkhd }, mvhd_path, PropertyPath::from("duration"));
                }
            }
    }
}

pub struct TrackDurationConsistencyRule;

impl ValidationRule for TrackDurationConsistencyRule {
    fn code(&self) -> &'static str {
        "TM003"
    }

    fn name(&self) -> &'static str {
        "track-duration-consistency"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let Some(mvhd_timescale) = state.mvhd_timescale else {
            return;
        };
        if mvhd_timescale == 0 {
            return;
        }

        let mut warnings: Vec<(u32, u64, u64, Option<BoxPath>)> = Vec::new();
        let mut overflows: Vec<(u32, u64, u32, Option<BoxPath>)> = Vec::new();
        for (&track_id, info) in &state.track_info {
            if info.has_edts {
                continue;
            }
            let Some(mdhd_duration) = info.mdhd_duration else {
                continue;
            };
            let Some(mdhd_timescale) = info.mdhd_timescale else {
                continue;
            };
            if mdhd_timescale == 0 || mdhd_duration == u64::MAX {
                continue;
            }

            let Some(expected) = mdhd_duration.checked_mul(mvhd_timescale as u64) else {
                overflows.push((track_id, mdhd_duration, mvhd_timescale, info.tkhd_path.clone()));
                continue;
            };
            let expected = expected / mdhd_timescale as u64;
            let actual = info.tkhd_duration;
            let diff = expected.abs_diff(actual);

            if diff > 1 {
                warnings.push((track_id, actual, expected, info.tkhd_path.clone()));
            }
        }

        for (track_id, mdhd_duration, mvhd_timescale, path) in overflows {
            ctx.emit_at(DiagnosticType::DurationComputationOverflow { track_id, mdhd_duration, mvhd_timescale }, path);
        }
        for (track_id, actual, expected, path) in warnings {
            ctx.emit_at_property(DiagnosticType::TrackDurationMismatch { track_id, actual, expected }, path, PropertyPath::from("duration"));
        }
    }
}

pub struct EditListDurationRule;

impl ValidationRule for EditListDurationRule {
    fn code(&self) -> &'static str {
        "TM004"
    }

    fn name(&self) -> &'static str {
        "edit-list-duration"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let mut warnings: Vec<(u32, u64, u64, Option<BoxPath>)> = Vec::new();
        for (&track_id, info) in &state.track_info {
            if info.edit_list_entries.is_empty() {
                continue;
            }
            let sum: u64 = info.edit_list_entries.iter().map(|&(d, _, _)| d).sum();
            let tkhd_dur = info.tkhd_duration;

            if sum != tkhd_dur && tkhd_dur != u64::MAX {
                warnings.push((track_id, sum, tkhd_dur, info.trak_path.clone()));
            }
        }

        for (track_id, sum, tkhd_dur, path) in warnings {
            ctx.emit_at(DiagnosticType::EditListDurationMismatch { track_id, sum, tkhd_duration: tkhd_dur }, path);
        }
    }
}

pub struct EditListMediaTimeRule;

impl ValidationRule for EditListMediaTimeRule {
    fn code(&self) -> &'static str {
        "TM005"
    }

    fn name(&self) -> &'static str {
        "edit-list-media-time"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::ELST {
            return;
        }

        let Ok(elst) = EditListBoxView::new(raw_box.data()) else {
            return;
        };

        for i in 0..elst.entry_count() as usize {
            if let Some(entry) = elst.entry(i)
                && entry.media_time < -1 {
                    ctx.emit_property(DiagnosticType::EditListInvalidMediaTime { entry: i, media_time: entry.media_time }, PropertyPath::from_indexed("entry", i).push("media_time"));
                }
        }
    }
}

pub struct EditListMediaRateRule;

impl ValidationRule for EditListMediaRateRule {
    fn code(&self) -> &'static str {
        "TM006"
    }

    fn name(&self) -> &'static str {
        "edit-list-media-rate"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::ELST {
            return;
        }

        let Ok(elst) = EditListBoxView::new(raw_box.data()) else {
            return;
        };

        for i in 0..elst.entry_count() as usize {
            if let Some(entry) = elst.entry(i) {
                let rate_raw = entry.media_rate.raw();
                if rate_raw != 0x00010000 && rate_raw != 0 {
                    if entry.media_time != -1 {
                        ctx.emit_property(DiagnosticType::EditListUnusualMediaRate { entry: i, rate_raw }, PropertyPath::from_indexed("entry", i).push("media_rate"));
                    }
                }
            }
        }
    }
}

pub struct IndefiniteDurationRule;

impl ValidationRule for IndefiniteDurationRule {
    fn code(&self) -> &'static str {
        "TM008"
    }

    fn name(&self) -> &'static str {
        "indefinite-duration"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let Some(mvhd_duration) = state.mvhd_duration else {
            return;
        };

        let any_indefinite = state.track_info.values().any(|info| {
            info.mdhd_duration == Some(u64::MAX) || info.mdhd_duration == Some(u32::MAX as u64)
        });

        if any_indefinite && mvhd_duration != u64::MAX && mvhd_duration != u32::MAX as u64 {
            ctx.emit(DiagnosticType::IndefiniteDurationInconsistent);
        }
    }
}

pub struct PtsStartEditListRule;

impl ValidationRule for PtsStartEditListRule {
    fn code(&self) -> &'static str {
        "TM010"
    }

    fn name(&self) -> &'static str {
        "pts-start-without-elst"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let mut warnings: Vec<(u32, i64, Option<BoxPath>)> = Vec::new();

        for (&track_id, info) in &state.track_info {
            if info.ctts_entries.is_empty() {
                continue;
            }

            let first_offset = info.ctts_entries[0].1;
            if first_offset == 0 {
                continue;
            }

            if !info.edit_list_entries.is_empty() {
                continue;
            }

            if let Some(timescale) = info.mdhd_timescale {
                if timescale > 0 && (first_offset.unsigned_abs() as f64 / timescale as f64) < 1.0 {
                    continue;
                }
            }

            warnings.push((track_id, first_offset, info.trak_path.clone()));
        }

        for (track_id, first_cts_offset, path) in warnings {
            ctx.emit_at(
                DiagnosticType::PtsStartWithoutEditList {
                    track_id,
                    first_cts_offset,
                },
                path,
            );
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::context::ValidationOptions;

    pub(crate) fn make_mvhd(timescale: u32) -> Vec<u8> {
        let mut data = Vec::new();

        data.extend_from_slice(&108u32.to_be_bytes());
        data.extend_from_slice(b"mvhd");
        data.push(0); // version
        data.extend_from_slice(&[0, 0, 0]); // flags

        data.extend_from_slice(&0u32.to_be_bytes()); // creation_time
        data.extend_from_slice(&0u32.to_be_bytes()); // modification_time
        data.extend_from_slice(&timescale.to_be_bytes()); // timescale
        data.extend_from_slice(&0u32.to_be_bytes()); // duration

        data.extend_from_slice(&0x00010000i32.to_be_bytes()); // rate = 1.0
        data.extend_from_slice(&0x0100i16.to_be_bytes()); // volume = 1.0

        data.extend_from_slice(&[0u8; 2]); // reserved
        data.extend_from_slice(&[0u8; 8]); // reserved

        let matrix = [0x10000i32, 0, 0, 0, 0x10000, 0, 0, 0, 0x10000];
        for m in matrix {
            data.extend_from_slice(&m.to_be_bytes());
        }

        data.extend_from_slice(&[0u8; 24]); // pre_defined
        data.extend_from_slice(&1u32.to_be_bytes()); // next_track_id

        data
    }

    pub(crate) fn make_mdhd(timescale: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&32u32.to_be_bytes()); // size
        data.extend_from_slice(b"mdhd");
        data.push(0); // version
        data.extend_from_slice(&[0, 0, 0]); // flags

        data.extend_from_slice(&0u32.to_be_bytes()); // creation_time
        data.extend_from_slice(&0u32.to_be_bytes()); // modification_time
        data.extend_from_slice(&timescale.to_be_bytes()); // timescale
        data.extend_from_slice(&0u32.to_be_bytes()); // duration
        data.extend_from_slice(&0x55C4u16.to_be_bytes()); // language = "und"
        data.extend_from_slice(&0u16.to_be_bytes()); // pre_defined

        data
    }

    #[test]
    fn valid_timescale() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        let mvhd = make_mvhd(48000);
        let raw = isobmff_syntax::RawBox::new(&mvhd).unwrap();
        TimescaleNonzeroRule.validate_box(&mut ctx, &raw);

        let mdhd = make_mdhd(48000);
        let raw = isobmff_syntax::RawBox::new(&mdhd).unwrap();
        TimescaleNonzeroRule.validate_box(&mut ctx, &raw);

        assert!(!ctx.has_errors());
    }

    #[test]
    fn zero_timescale() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mvhd = make_mvhd(0);
        let raw = isobmff_syntax::RawBox::new(&mvhd).unwrap();
        TimescaleNonzeroRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
        assert!(ctx.diagnostics()[0].message().contains("mvhd"));

        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mdhd = make_mdhd(0);
        let raw = isobmff_syntax::RawBox::new(&mdhd).unwrap();
        TimescaleNonzeroRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
        assert!(ctx.diagnostics()[0].message().contains("mdhd"));
    }

    pub(crate) fn make_mvhd_with_next_track_id(timescale: u32, next_track_id: u32) -> Vec<u8> {
        let mut data = make_mvhd(timescale);
        let len = data.len();
        data[len - 4..].copy_from_slice(&next_track_id.to_be_bytes());
        data
    }

    #[test]
    fn m003_nonzero_next_track_id() {
        let mvhd = make_mvhd_with_next_track_id(48000, 2);
        let raw = isobmff_syntax::RawBox::new(&mvhd).unwrap();
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        MvhdNextTrackIdNonzeroRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn m003_zero_next_track_id() {
        let mvhd = make_mvhd_with_next_track_id(48000, 0);
        let raw = isobmff_syntax::RawBox::new(&mvhd).unwrap();
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        MvhdNextTrackIdNonzeroRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }

    #[test]
    fn m004_next_track_id_greater() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().next_track_id = Some(3);
        ctx.state_mut().seen_track_ids.insert(1);
        ctx.state_mut().seen_track_ids.insert(2);
        MvhdNextTrackIdGreaterRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn m004_next_track_id_too_small() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().next_track_id = Some(1);
        ctx.state_mut().seen_track_ids.insert(1);
        ctx.state_mut().seen_track_ids.insert(2);
        MvhdNextTrackIdGreaterRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn m004_max_u32_ok() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().next_track_id = Some(0xFFFFFFFF);
        ctx.state_mut().seen_track_ids.insert(1);
        MvhdNextTrackIdGreaterRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn m005_valid_version() {
        let mvhd = make_mvhd(48000);
        let raw = isobmff_syntax::RawBox::new(&mvhd).unwrap();
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        MvhdVersionRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn m005_invalid_version() {
        let mut mvhd = make_mvhd(48000);
        mvhd[8] = 2;
        let raw = isobmff_syntax::RawBox::new(&mvhd).unwrap();
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        MvhdVersionRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }

    fn make_elst(entries: &[(u64, i64, i32)]) -> Vec<u8> {
        let entry_size = 8 + 8 + 4; // segment_duration(8) + media_time(8) + media_rate(4)
        let size = 8 + 4 + 4 + entries.len() * entry_size;
        let mut data = Vec::with_capacity(size);
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"elst");
        data.push(1); // version 1
        data.extend_from_slice(&[0, 0, 0]); // flags
        data.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for &(duration, media_time, rate) in entries {
            data.extend_from_slice(&duration.to_be_bytes());
            data.extend_from_slice(&media_time.to_be_bytes());
            data.extend_from_slice(&rate.to_be_bytes());
        }
        data
    }

    #[test]
    fn tm005_valid_media_time() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let elst = make_elst(&[(1000, 0, 0x00010000)]);
        let raw = isobmff_syntax::RawBox::new(&elst).unwrap();
        EditListMediaTimeRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn tm005_empty_edit_ok() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let elst = make_elst(&[(1000, -1, 0x00010000)]);
        let raw = isobmff_syntax::RawBox::new(&elst).unwrap();
        EditListMediaTimeRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn tm005_invalid_media_time() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let elst = make_elst(&[(1000, -2, 0x00010000)]);
        let raw = isobmff_syntax::RawBox::new(&elst).unwrap();
        EditListMediaTimeRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }

    #[test]
    fn tm004_duration_match() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut info = crate::context::TrackInfo::default();
        info.tkhd_duration = 2000;
        info.edit_list_entries = vec![(1000, 0, 0x00010000), (1000, 500, 0x00010000)];
        ctx.state_mut().track_info.insert(1, info);
        EditListDurationRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn tm004_duration_mismatch() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut info = crate::context::TrackInfo::default();
        info.tkhd_duration = 3000;
        info.edit_list_entries = vec![(1000, 0, 0x00010000), (1000, 500, 0x00010000)];
        ctx.state_mut().track_info.insert(1, info);
        EditListDurationRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn tm008_indefinite_consistency() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().mvhd_duration = Some(1000);
        let mut info = crate::context::TrackInfo::default();
        info.mdhd_duration = Some(u64::MAX);
        ctx.state_mut().track_info.insert(1, info);
        IndefiniteDurationRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn m004_lenient_skips() {
        let mut ctx = ValidationContext::new(ValidationOptions::new().lenient(true));
        ctx.state_mut().next_track_id = Some(1);
        ctx.state_mut().seen_track_ids.insert(2);
        MvhdNextTrackIdGreaterRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn tm009_duration_computation_overflow() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().mvhd_timescale = Some(48000);
        let mut info = crate::context::TrackInfo::default();
        info.mdhd_duration = Some(u64::MAX - 1); // not u64::MAX (that's indefinite)
        info.mdhd_timescale = Some(44100);
        info.tkhd_duration = 1000;
        ctx.state_mut().track_info.insert(1, info);

        TrackDurationConsistencyRule.finalize(&mut ctx);
        assert!(ctx.diagnostics().iter().any(|d| d.code() == "TM009"),
            "Expected TM009 diagnostic for duration computation overflow");
    }
}
