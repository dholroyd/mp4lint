
use crate::context::ValidationContext;
use crate::diagnostic::{BoxPath, DiagnosticType, PropertyPath, SampleFlagsLocation};
use crate::rules::ValidationRule;
use isobmff_syntax::boxes::{
    MovieFragmentRandomAccessOffsetBox, MovieFragmentRandomAccessOffsetBoxView,
    TrackExtendsBox, TrackExtendsBoxView,
    TrackFragmentHeaderBox, TrackFragmentHeaderBoxView,
};
use isobmff_syntax::{BoxCode, BoxInfo, BrandCode, RawBox};

pub struct MoofMfhdRequiredRule;

impl ValidationRule for MoofMfhdRequiredRule {
    fn code(&self) -> &'static str {
        "MF001"
    }

    fn name(&self) -> &'static str {
        "moof-mfhd-required"
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::MOOF {
            return;
        }

        let mfhd_count = ctx.current_container_tracker()
            .map(|t| t.child_count(BoxCode::MFHD))
            .unwrap_or(0);

        if mfhd_count == 0 {
            ctx.emit(DiagnosticType::MoofMfhdMissing);
        } else if mfhd_count > 1 {
            ctx.emit(DiagnosticType::MoofMfhdDuplicate { count: mfhd_count });
        }
    }
}

pub struct SequenceNumberOrderRule;

impl ValidationRule for SequenceNumberOrderRule {
    fn code(&self) -> &'static str {
        "MF002"
    }

    fn name(&self) -> &'static str {
        "sequence-number-order"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let seq = &ctx.state().moof_sequence_numbers;
        if seq.is_empty() {
            return;
        }
        for (i, &num) in seq.iter().enumerate() {
            let expected = (i + 1) as u32;
            if num != expected {
                ctx.emit(DiagnosticType::SequenceNumberOutOfOrder { value: num, position: i, expected });
                break;
            }
        }
    }
}

pub struct MoofRequiresMvexRule;

impl ValidationRule for MoofRequiresMvexRule {
    fn code(&self) -> &'static str {
        "MF003"
    }

    fn name(&self) -> &'static str {
        "moof-requires-mvex"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        if ctx.state().has_moof && !ctx.state().has_mvex {
            ctx.emit(DiagnosticType::MoofWithoutMvex);
        }
    }
}

pub struct TrexPerTrackRule;

impl ValidationRule for TrexPerTrackRule {
    fn code(&self) -> &'static str {
        "MF004"
    }

    fn name(&self) -> &'static str {
        "trex-per-track"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        if !state.has_mvex {
            return;
        }
        let missing: Vec<(u32, Option<BoxPath>)> = state.seen_track_ids.iter()
            .filter(|id| !state.trex_track_ids.contains(id))
            .map(|&id| {
                let path = state.track_info.get(&id).and_then(|i| i.trak_path.clone());
                (id, path)
            })
            .collect();
        for (track_id, path) in missing {
            ctx.emit_at(DiagnosticType::TrackMissingTrex { track_id }, path);
        }
    }
}

pub struct TrexTrackIdValidRule;

impl ValidationRule for TrexTrackIdValidRule {
    fn code(&self) -> &'static str {
        "MF005"
    }

    fn name(&self) -> &'static str {
        "trex-track-id-valid"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let mvex_path = state.mvex_path.clone();
        let invalid: Vec<u32> = state.trex_track_ids.iter()
            .filter(|id| !state.seen_track_ids.contains(id))
            .copied()
            .collect();
        for trex_id in invalid {
            ctx.emit_at(DiagnosticType::TrexTrackIdNotFound { track_id: trex_id }, mvex_path.clone());
        }
    }
}

pub struct TrafTfhdRequiredRule;

impl ValidationRule for TrafTfhdRequiredRule {
    fn code(&self) -> &'static str {
        "MF006"
    }

    fn name(&self) -> &'static str {
        "traf-tfhd-required"
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::TRAF {
            return;
        }

        let tfhd_count = ctx.current_container_tracker()
            .map(|t| t.child_count(BoxCode::TFHD))
            .unwrap_or(0);

        if tfhd_count == 0 {
            ctx.emit(DiagnosticType::TrafTfhdMissing);
        } else if tfhd_count > 1 {
            ctx.emit(DiagnosticType::TrafTfhdDuplicate { count: tfhd_count });
        }
    }
}

pub struct TfhdTrackIdValidRule;

impl ValidationRule for TfhdTrackIdValidRule {
    fn code(&self) -> &'static str {
        "MF007"
    }

    fn name(&self) -> &'static str {
        "tfhd-track-id-valid"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::TFHD {
            return;
        }

        let Ok(tfhd) = TrackFragmentHeaderBoxView::new(raw_box.data()) else {
            return;
        };

        let track_id = tfhd.track_id();
        let state = ctx.state();
        if !state.seen_track_ids.contains(&track_id) {
            ctx.emit_property(DiagnosticType::TfhdTrackIdInvalid { track_id }, PropertyPath::from("track_id"));
        }
    }
}

pub struct TfdtOrderRule;

impl ValidationRule for TfdtOrderRule {
    fn code(&self) -> &'static str {
        "MF008"
    }

    fn name(&self) -> &'static str {
        "tfdt-order"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::TRAF {
            return;
        }

        let diagnostics: Vec<DiagnosticType> = {
            let Some(tracker) = ctx.current_container_tracker() else {
                return;
            };
            let mut msgs = Vec::new();
            let mut found_tfhd = false;
            let mut found_trun = false;

            for child_type in &tracker.children {
                match *child_type {
                    BoxCode::TFHD => found_tfhd = true,
                    BoxCode::TFDT => {
                        if !found_tfhd {
                            msgs.push(DiagnosticType::TfdtBeforeTfhd);
                        }
                        if found_trun {
                            msgs.push(DiagnosticType::TfdtAfterTrun);
                        }
                    }
                    BoxCode::TRUN => found_trun = true,
                    _ => {}
                }
            }
            msgs
        };

        for dt in diagnostics {
            ctx.emit(dt);
        }
    }
}

pub struct FragmentDurationRule;

impl ValidationRule for FragmentDurationRule {
    fn code(&self) -> &'static str {
        "MF009"
    }

    fn name(&self) -> &'static str {
        "fragment-duration"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        if state.has_mvex && !state.has_mehd {
            let mvex_path = state.mvex_path.clone();
            ctx.emit_at(DiagnosticType::MvexWithoutMehd, mvex_path);
        }
    }
}

pub struct DurationIsEmptyWithEdtsRule;

impl ValidationRule for DurationIsEmptyWithEdtsRule {
    fn code(&self) -> &'static str {
        "MF010"
    }

    fn name(&self) -> &'static str {
        "duration-is-empty-with-edts"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::TFHD {
            return;
        }

        let Ok(tfhd) = TrackFragmentHeaderBoxView::new(raw_box.data()) else {
            return;
        };

        let duration_is_empty = tfhd.flags() & 0x010000 != 0;
        if !duration_is_empty {
            return;
        }

        let track_id = tfhd.track_id();
        if ctx.state().tracks_with_edts.contains(&track_id) {
            ctx.emit_property(DiagnosticType::DurationIsEmptyWithEdts { track_id }, PropertyPath::from("flags"));
        }
    }
}

pub struct BaseDataOffsetConsistencyRule;

impl ValidationRule for BaseDataOffsetConsistencyRule {
    fn code(&self) -> &'static str {
        "MF011"
    }

    fn name(&self) -> &'static str {
        "base-data-offset-consistency"
    }

    fn enter_container(&self, ctx: &mut ValidationContext, box_type: BoxCode, _info: &BoxInfo) {
        if box_type == BoxCode::MOOF {
            ctx.state_mut().current_moof_base_offsets.clear();
            ctx.state_mut().current_moof_traf_modes.clear();
        }
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::TFHD {
            return;
        }

        let Ok(tfhd) = TrackFragmentHeaderBoxView::new(raw_box.data()) else {
            return;
        };

        let base_offset_present = tfhd.flags() & 0x000001 != 0;
        let default_base_is_moof = tfhd.flags() & 0x020000 != 0;

        ctx.state_mut()
            .current_moof_traf_modes
            .push((base_offset_present, default_base_is_moof));

        if base_offset_present && !default_base_is_moof {
            ctx.state_mut()
                .current_moof_base_offsets
                .push(tfhd.base_data_offset());
        }
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::MOOF {
            return;
        }

        let modes = &ctx.state().current_moof_traf_modes;
        if modes.len() < 2 {
            return;
        }

        let has_explicit = modes.iter().any(|(bop, _)| *bop);
        let has_moof_relative = modes.iter().any(|(_, dbim)| *dbim);
        let has_implicit = modes.iter().any(|(bop, dbim)| !*bop && !*dbim);

        if has_explicit && has_implicit {
            ctx.emit(DiagnosticType::MoofMixedAddressing);
        }
        if has_moof_relative && has_implicit {
            ctx.emit(DiagnosticType::MoofMixedMoofRelative);
        }
    }
}

pub struct SampleFlagsDefinedBitsRule;

impl ValidationRule for SampleFlagsDefinedBitsRule {
    fn code(&self) -> &'static str {
        "MF012"
    }

    fn name(&self) -> &'static str {
        "sample-flags-defined-bits"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        isobmff_syntax::dispatch_box!(raw_box, {
            TrackFragmentHeaderBoxView(tfhd) => {
                if let Some(flags) = tfhd.default_sample_flags() {
                    self.check_flags(ctx, flags, SampleFlagsLocation::TfhdDefault, PropertyPath::from("default_sample_flags"));
                }
            },
            TrackExtendsBoxView(trex) => {
                let flags = trex.default_sample_flags();
                self.check_flags(ctx, flags, SampleFlagsLocation::TrexDefault, PropertyPath::from("default_sample_flags"));
            },
            _ => {},
            Err(_) => {}
        });
    }
}

impl SampleFlagsDefinedBitsRule {
    fn check_flags(&self, ctx: &mut ValidationContext, flags: u32, location: SampleFlagsLocation, property: PropertyPath) {
        if flags & 0x0000000F != 0 {
            ctx.emit_property(DiagnosticType::SampleFlagsReservedBits { location }, property);
        }
    }
}

pub struct MfraPositionRule;

impl ValidationRule for MfraPositionRule {
    fn code(&self) -> &'static str {
        "MF013"
    }

    fn name(&self) -> &'static str {
        "mfra-position"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        if let (Some(offset), Some(isobmff_syntax::BoxSize::Known(size))) = (state.mfra_offset, state.mfra_size) {
            let Some(end) = offset.checked_add(size.get()) else {
                ctx.emit(DiagnosticType::BoxExtentOverflow { offset, size: size.get() });
                return;
            };
            if end != state.file_size {
                ctx.emit(DiagnosticType::MfraNotAtEnd { mfra_end: end, file_size: state.file_size });
            }
        } else if let (Some(_offset), Some(isobmff_syntax::BoxSize::ToEnd)) = (state.mfra_offset, state.mfra_size) {
        }
    }
}

pub struct MfroRequiredRule;

impl ValidationRule for MfroRequiredRule {
    fn code(&self) -> &'static str {
        "MF014"
    }

    fn name(&self) -> &'static str {
        "mfro-required"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::MFRO {
            return;
        }

        let Ok(mfro) = MovieFragmentRandomAccessOffsetBoxView::new(raw_box.data()) else {
            return;
        };

        let mfra_size = ctx.state().container_stack.iter().rev()
            .find(|(bt, _)| *bt == BoxCode::MFRA)
            .and_then(|(_, tracker)| {
                if let isobmff_syntax::BoxSize::Known(n) = tracker.box_info_size {
                    Some(n.get())
                } else {
                    None
                }
            });

        if let Some(actual) = mfra_size {
            let declared = mfro.mfra_size() as u64;
            if declared != actual {
                ctx.emit_property(DiagnosticType::MfroSizeMismatch { declared, actual }, PropertyPath::from("mfra_size"));
            }
        }
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::MFRA {
            return;
        }

        let has_mfro = ctx.current_container_tracker()
            .map(|t| t.has_child(BoxCode::MFRO))
            .unwrap_or(true);

        if !has_mfro {
            ctx.emit(DiagnosticType::MfraMissingMfro);
        }
    }
}

pub struct DefaultBaseIsMoofBrandRule;

impl ValidationRule for DefaultBaseIsMoofBrandRule {
    fn code(&self) -> &'static str {
        "MF015"
    }

    fn name(&self) -> &'static str {
        "default-base-is-moof-brand"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::TFHD {
            return;
        }

        let Ok(tfhd) = TrackFragmentHeaderBoxView::new(raw_box.data()) else {
            return;
        };

        if tfhd.flags() & 0x020000 == 0 {
            return;
        }

        let iso5_brands: &[BrandCode] = &[
            BrandCode::ISO5, BrandCode::ISO6, BrandCode::ISO7,
            BrandCode::ISO8, BrandCode::ISO9, BrandCode::ISOA,
            BrandCode::ISOB, BrandCode::ISOC,
        ];
        let has_iso5 = ctx
            .state()
            .compatible_brands
            .iter()
            .any(|b| iso5_brands.contains(b));
        if !has_iso5 {
            ctx.emit_property(DiagnosticType::DefaultBaseIsMoofWithoutBrand, PropertyPath::from("flags"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ValidationOptions;

    #[test]
    fn mf001_moof_with_mfhd() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = crate::context::ContainerChildTracker::default();
        tracker.record_child(BoxCode::MFHD);
        ctx.state_mut().container_stack.push((BoxCode::MOOF, tracker));
        MoofMfhdRequiredRule.exit_container(&mut ctx, BoxCode::MOOF);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn mf001_moof_without_mfhd() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let tracker = crate::context::ContainerChildTracker::default();
        ctx.state_mut().container_stack.push((BoxCode::MOOF, tracker));
        MoofMfhdRequiredRule.exit_container(&mut ctx, BoxCode::MOOF);
        assert!(ctx.has_errors());
    }

    #[test]
    fn mf002_sequence_ok() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().moof_sequence_numbers = vec![1, 2, 3];
        SequenceNumberOrderRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn mf002_sequence_bad() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().moof_sequence_numbers = vec![1, 3, 2];
        SequenceNumberOrderRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn mf003_moof_no_mvex() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().has_moof = true;
        ctx.state_mut().has_mvex = false;
        MoofRequiresMvexRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
    }

    #[test]
    fn mf003_moof_with_mvex() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().has_moof = true;
        ctx.state_mut().has_mvex = true;
        MoofRequiresMvexRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn mf006_traf_with_tfhd() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = crate::context::ContainerChildTracker::default();
        tracker.record_child(BoxCode::TFHD);
        ctx.state_mut().container_stack.push((BoxCode::TRAF, tracker));
        TrafTfhdRequiredRule.exit_container(&mut ctx, BoxCode::TRAF);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn mf006_traf_without_tfhd() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let tracker = crate::context::ContainerChildTracker::default();
        ctx.state_mut().container_stack.push((BoxCode::TRAF, tracker));
        TrafTfhdRequiredRule.exit_container(&mut ctx, BoxCode::TRAF);
        assert!(ctx.has_errors());
    }

    #[test]
    fn mf013_mfra_at_end() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().mfra_offset = Some(900);
        ctx.state_mut().mfra_size = Some(isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(100).unwrap()));
        ctx.state_mut().file_size = 1000;
        MfraPositionRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn mf013_mfra_not_at_end() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().mfra_offset = Some(500);
        ctx.state_mut().mfra_size = Some(isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(100).unwrap()));
        ctx.state_mut().file_size = 1000;
        MfraPositionRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 1);
    }


    #[test]
    fn mf011_consistent_default_base_moof() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().current_moof_traf_modes = vec![(false, true), (false, true)];
        BaseDataOffsetConsistencyRule.exit_container(&mut ctx, BoxCode::MOOF);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn mf011_mixed_explicit_and_implicit() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().current_moof_traf_modes = vec![(true, false), (false, false)];
        BaseDataOffsetConsistencyRule.exit_container(&mut ctx, BoxCode::MOOF);
        assert!(ctx.has_errors());
    }

    #[test]
    fn mf011_single_traf_no_error() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().current_moof_traf_modes = vec![(true, false)];
        BaseDataOffsetConsistencyRule.exit_container(&mut ctx, BoxCode::MOOF);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn f009_mfra_extent_overflow() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().mfra_offset = Some(u64::MAX);
        ctx.state_mut().mfra_size = Some(isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(1).unwrap()));
        ctx.state_mut().file_size = 1000;
        MfraPositionRule.finalize(&mut ctx);
        assert!(ctx.diagnostics().iter().any(|d| d.code() == "F009"),
            "Expected F009 diagnostic for box extent overflow");
    }
}
