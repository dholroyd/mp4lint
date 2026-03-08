
use crate::context::ValidationContext;
use crate::diagnostic::{BoxPath, DiagnosticType};
use crate::rules::ValidationRule;
use isobmff_syntax::boxes::{TrackReferenceTypeBox, TrackReferenceTypeBoxView};
use isobmff_syntax::{BoxCode, HandlerCode, RawBox, TrackReferenceCode};
use std::collections::HashSet;

fn get_tref_child_ref_type(raw_box: &RawBox<'_>) -> TrackReferenceCode {
    TrackReferenceCode(raw_box.box_type().0)
}

pub struct ReferenceTypeDefinedRule;

impl ValidationRule for ReferenceTypeDefinedRule {
    fn code(&self) -> &'static str {
        "TR001"
    }

    fn name(&self) -> &'static str {
        "reference-type-defined"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if !ctx.path().contains(BoxCode::TREF) {
            return;
        }
        if raw_box.box_type() == BoxCode::TREF {
            return;
        }
        let rt = get_tref_child_ref_type(raw_box);

        let known: &[TrackReferenceCode] = &[
            TrackReferenceCode::HINT, TrackReferenceCode::CDSC,
            TrackReferenceCode::FONT, TrackReferenceCode::HIND,
            TrackReferenceCode::VDEP, TrackReferenceCode::VPLX,
            TrackReferenceCode::SUBT, TrackReferenceCode::THMB,
            TrackReferenceCode::AUXL, TrackReferenceCode::CDTG,
            TrackReferenceCode::SHSC,
            TrackReferenceCode::TMCD, TrackReferenceCode::new(*b"chap"),
            TrackReferenceCode::SYNC, TrackReferenceCode::new(*b"scpt"),
            TrackReferenceCode::new(*b"ssrc"),
        ];
        if !known.contains(&rt) {
            ctx.emit(DiagnosticType::UnknownTrackRefType { ref_type: rt.0 });
        }
    }
}

pub struct NoDuplicateTrackIdsInRefRule;

impl ValidationRule for NoDuplicateTrackIdsInRefRule {
    fn code(&self) -> &'static str {
        "TR002"
    }

    fn name(&self) -> &'static str {
        "no-duplicate-track-ids-in-ref"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if !ctx.path().contains(BoxCode::TREF) {
            return;
        }
        if raw_box.box_type() == BoxCode::TREF {
            return;
        }
        let rt = get_tref_child_ref_type(raw_box);

        let Ok(tref_type) = TrackReferenceTypeBoxView::new(raw_box.data()) else {
            return;
        };

        let mut seen = HashSet::new();
        for id in tref_type.track_ids() {
            if !seen.insert(id) {
                ctx.emit(DiagnosticType::DuplicateTrackIdInRef {
                    track_id: id,
                    ref_type: rt.0,
                });
            }
        }
    }
}

pub struct NoZeroTrackIdsInRefRule;

impl ValidationRule for NoZeroTrackIdsInRefRule {
    fn code(&self) -> &'static str {
        "TR003"
    }

    fn name(&self) -> &'static str {
        "no-zero-track-ids-in-ref"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if !ctx.path().contains(BoxCode::TREF) {
            return;
        }
        if raw_box.box_type() == BoxCode::TREF {
            return;
        }
        let rt = get_tref_child_ref_type(raw_box);

        let Ok(tref_type) = TrackReferenceTypeBoxView::new(raw_box.data()) else {
            return;
        };

        for id in tref_type.track_ids() {
            if id == 0 {
                ctx.emit(DiagnosticType::ZeroTrackIdInRef { ref_type: rt.0 });
            }
        }
    }
}

pub struct ReferencedTrackIdsExistRule;

impl ValidationRule for ReferencedTrackIdsExistRule {
    fn code(&self) -> &'static str {
        "TR004"
    }

    fn name(&self) -> &'static str {
        "referenced-track-ids-exist"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let missing: Vec<(u32, TrackReferenceCode)> = {
            let state = ctx.state();
            state
                .referenced_track_ids
                .iter()
                .filter(|&&(ref_id, _)| ref_id != 0 && !state.seen_track_ids.contains(&ref_id))
                .copied()
                .collect()
        };
        for (ref_id, ref_type) in missing {
            ctx.emit(DiagnosticType::ReferencedTrackNotFound {
                ref_id,
                ref_type: ref_type.0,
            });
        }
    }
}

pub struct HintTrackReferenceRule;

impl ValidationRule for HintTrackReferenceRule {
    fn code(&self) -> &'static str {
        "TR005"
    }

    fn name(&self) -> &'static str {
        "hint-track-reference"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let hint_tracks_without_ref: Vec<(u32, Option<BoxPath>)> = {
            let state = ctx.state();
            state
                .track_info
                .iter()
                .filter(|(_, info)| info.handler_type == Some(HandlerCode::HINT) && !info.has_hint_tref)
                .map(|(&track_id, info)| (track_id, info.trak_path.clone()))
                .collect()
        };
        for (track_id, path) in hint_tracks_without_ref {
            ctx.emit_at(DiagnosticType::HintTrackMissingRef { track_id }, path);
        }
    }
}

pub struct ReservedTrackRefTypesRule;

impl ValidationRule for ReservedTrackRefTypesRule {
    fn code(&self) -> &'static str {
        "F007b"
    }

    fn name(&self) -> &'static str {
        "reserved-track-ref-types"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if !ctx.path().contains(BoxCode::TREF) {
            return;
        }
        if raw_box.box_type() == BoxCode::TREF {
            return;
        }
        let rt = get_tref_child_ref_type(raw_box);

        let reserved: &[TrackReferenceCode] = &[
            TrackReferenceCode::TMCD, TrackReferenceCode::new(*b"chap"),
            TrackReferenceCode::SYNC, TrackReferenceCode::new(*b"scpt"),
            TrackReferenceCode::new(*b"ssrc"),
        ];
        if reserved.contains(&rt) {
            ctx.emit(DiagnosticType::ReservedTrackRefType { ref_type: rt.0 });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ValidationOptions;

    fn make_tref_child(ref_type: &[u8; 4], track_ids: &[u32]) -> Vec<u8> {
        let size = 8 + track_ids.len() * 4;
        let mut data = Vec::with_capacity(size);
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(ref_type);
        for &id in track_ids {
            data.extend_from_slice(&id.to_be_bytes());
        }
        data
    }

    #[test]
    fn tr001_known_type() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.push_path(BoxCode::TREF);
        let tref_child = make_tref_child(b"hint", &[1]);
        let raw = RawBox::new(&tref_child).unwrap();
        ReferenceTypeDefinedRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn tr001_unknown_type() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.push_path(BoxCode::TREF);
        let tref_child = make_tref_child(b"zzzz", &[1]);
        let raw = RawBox::new(&tref_child).unwrap();
        ReferenceTypeDefinedRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn tr002_no_duplicates() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.push_path(BoxCode::TREF);
        let tref_child = make_tref_child(b"hint", &[1, 2, 3]);
        let raw = RawBox::new(&tref_child).unwrap();
        NoDuplicateTrackIdsInRefRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn tr002_with_duplicates() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.push_path(BoxCode::TREF);
        let tref_child = make_tref_child(b"hint", &[1, 2, 1]);
        let raw = RawBox::new(&tref_child).unwrap();
        NoDuplicateTrackIdsInRefRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }

    #[test]
    fn tr003_no_zeros() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.push_path(BoxCode::TREF);
        let tref_child = make_tref_child(b"hint", &[1, 2]);
        let raw = RawBox::new(&tref_child).unwrap();
        NoZeroTrackIdsInRefRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn tr003_with_zero() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.push_path(BoxCode::TREF);
        let tref_child = make_tref_child(b"hint", &[0, 1]);
        let raw = RawBox::new(&tref_child).unwrap();
        NoZeroTrackIdsInRefRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }

    #[test]
    fn tr004_all_exist() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().seen_track_ids.insert(1);
        ctx.state_mut().seen_track_ids.insert(2);
        ctx.state_mut().referenced_track_ids.push((1, TrackReferenceCode::HINT));
        ctx.state_mut().referenced_track_ids.push((2, TrackReferenceCode::CDSC));
        ReferencedTrackIdsExistRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn tr004_missing_reference() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().seen_track_ids.insert(1);
        ctx.state_mut().referenced_track_ids.push((99, TrackReferenceCode::HINT));
        ReferencedTrackIdsExistRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn f007b_reserved_type() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.push_path(BoxCode::TREF);
        let tref_child = make_tref_child(b"tmcd", &[1]);
        let raw = RawBox::new(&tref_child).unwrap();
        ReservedTrackRefTypesRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn f007b_non_reserved_type() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.push_path(BoxCode::TREF);
        let tref_child = make_tref_child(b"hint", &[1]);
        let raw = RawBox::new(&tref_child).unwrap();
        ReservedTrackRefTypesRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }
}
