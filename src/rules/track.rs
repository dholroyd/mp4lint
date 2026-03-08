
use crate::context::ValidationContext;
use crate::diagnostic::{DiagnosticType, PropertyPath};
use crate::rules::ValidationRule;
use isobmff_syntax::boxes::{
    HandlerReferenceBox, HandlerReferenceBoxView,
    TrackHeaderBox, TrackHeaderBoxView,
    TrackReferenceBoxView,
};
use isobmff_syntax::{BoxCode, FourCC, FullBoxHeader, HandlerCode, RawBox};

pub struct MediaTrackPresenceRule;

impl ValidationRule for MediaTrackPresenceRule {
    fn code(&self) -> &'static str {
        "T002"
    }

    fn name(&self) -> &'static str {
        "media-track-presence"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::HDLR {
            return;
        }

        let Ok(hdlr) = HandlerReferenceBoxView::new(raw_box.data()) else {
            return;
        };

        if hdlr.handler_type() != HandlerCode::HINT {
            ctx.state_mut().has_media_track = true;
        }
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        if !ctx.state().has_media_track {
            ctx.emit(DiagnosticType::NoMediaTrack);
        }
    }
}

pub struct TrackIdUniquenessRule;

impl ValidationRule for TrackIdUniquenessRule {
    fn code(&self) -> &'static str {
        "T004"
    }

    fn name(&self) -> &'static str {
        "track-id-uniqueness"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::TKHD {
            return;
        }

        let Ok(tkhd) = TrackHeaderBoxView::new(raw_box.data()) else {
            return;
        };

        let track_id = tkhd.track_id();

        if track_id == 0 {
            ctx.emit_property(DiagnosticType::TrackIdZero, PropertyPath::from("track_id"));
        } else if !ctx.state_mut().seen_track_ids.insert(track_id) {
            ctx.emit_property(DiagnosticType::TrackIdDuplicate { track_id }, PropertyPath::from("track_id"));
        }
    }
}

pub struct TkhdVersionRule;

impl ValidationRule for TkhdVersionRule {
    fn code(&self) -> &'static str {
        "T007"
    }

    fn name(&self) -> &'static str {
        "tkhd-version"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::TKHD {
            return;
        }

        let Ok(header) = FullBoxHeader::parse(raw_box.data(), raw_box.data().len()) else {
            return;
        };
        if header.version > 1 {
            ctx.emit_property(DiagnosticType::TkhdInvalidVersion { version: header.version }, PropertyPath::from("version"));
        }
    }
}

pub struct TkhdFlagsRule;

impl ValidationRule for TkhdFlagsRule {
    fn code(&self) -> &'static str {
        "T008"
    }

    fn name(&self) -> &'static str {
        "tkhd-flags"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::TKHD {
            return;
        }

        let Ok(tkhd) = TrackHeaderBoxView::new(raw_box.data()) else {
            return;
        };

        let flags = tkhd.flags();
        let defined_mask: u32 = 0x00000F;
        if flags & !defined_mask != 0 {
            ctx.emit_property(DiagnosticType::TkhdUndefinedFlags {
                undefined_bits: flags & !defined_mask,
            }, PropertyPath::from("flags"));
        }
    }
}

pub struct TrefUniquenessRule;

impl ValidationRule for TrefUniquenessRule {
    fn code(&self) -> &'static str {
        "T009"
    }

    fn name(&self) -> &'static str {
        "tref-uniqueness"
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::TRAK {
            return;
        }

        let tref_count = ctx.current_container_tracker()
            .map(|t| t.child_count(BoxCode::TREF))
            .unwrap_or(0);

        if tref_count > 1 {
            ctx.emit(DiagnosticType::TrefDuplicate { count: tref_count });
        }
    }
}

pub struct TrefNoDuplicateTypesRule;

impl ValidationRule for TrefNoDuplicateTypesRule {
    fn code(&self) -> &'static str {
        "T010"
    }

    fn name(&self) -> &'static str {
        "tref-no-duplicate-types"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::TREF {
            return;
        }

        let Ok(tref) = TrackReferenceBoxView::new(raw_box.data()) else {
            return;
        };

        let mut seen_types = std::collections::HashSet::<FourCC>::new();
        for child in tref.children() {
            let bt = child.box_type();
            if !seen_types.insert(bt.0) {
                ctx.emit(DiagnosticType::TrefDuplicateType { ref_type: bt.0 });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ValidationOptions;

    fn make_box(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let size = 8 + payload.len();
        let mut data = Vec::with_capacity(size);
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(box_type);
        data.extend_from_slice(payload);
        data
    }

    fn make_tkhd_v0(track_id: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&92u32.to_be_bytes()); // size
        data.extend_from_slice(b"tkhd");
        data.push(0); // version
        data.extend_from_slice(&[0, 0, 0x07]); // flags

        data.extend_from_slice(&0u32.to_be_bytes()); // creation_time
        data.extend_from_slice(&0u32.to_be_bytes()); // modification_time
        data.extend_from_slice(&track_id.to_be_bytes()); // track_id
        data.extend_from_slice(&0u32.to_be_bytes()); // reserved
        data.extend_from_slice(&0u32.to_be_bytes()); // duration

        data.extend_from_slice(&[0u8; 8]); // reserved
        data.extend_from_slice(&0i16.to_be_bytes()); // layer
        data.extend_from_slice(&0i16.to_be_bytes()); // alternate_group
        data.extend_from_slice(&0x0100i16.to_be_bytes()); // volume = 1.0
        data.extend_from_slice(&0u16.to_be_bytes()); // reserved

        let matrix = [0x10000i32, 0, 0, 0, 0x10000, 0, 0, 0, 0x10000];
        for m in matrix {
            data.extend_from_slice(&m.to_be_bytes());
        }

        data.extend_from_slice(&0i32.to_be_bytes()); // width
        data.extend_from_slice(&0i32.to_be_bytes()); // height

        data
    }

    fn make_hdlr(handler_type: &[u8; 4]) -> Vec<u8> {
        let mut data = Vec::new();
        let size = 8 + 4 + 4 + 4 + 12 + 1;
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"hdlr");
        data.extend_from_slice(&[0, 0, 0, 0]); // version + flags
        data.extend_from_slice(&0u32.to_be_bytes()); // pre_defined
        data.extend_from_slice(handler_type); // handler_type
        data.extend_from_slice(&[0u8; 12]); // reserved
        data.push(0); // null-terminated name
        data
    }

    #[test]
    fn unique_track_ids() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        let tkhd1 = make_tkhd_v0(1);
        let raw1 = isobmff_syntax::RawBox::new(&tkhd1).unwrap();
        TrackIdUniquenessRule.validate_box(&mut ctx, &raw1);

        let tkhd2 = make_tkhd_v0(2);
        let raw2 = isobmff_syntax::RawBox::new(&tkhd2).unwrap();
        TrackIdUniquenessRule.validate_box(&mut ctx, &raw2);

        assert!(!ctx.has_errors());
        assert_eq!(ctx.state().seen_track_ids.len(), 2);
    }

    #[test]
    fn duplicate_track_ids() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        let tkhd1 = make_tkhd_v0(1);
        let raw1 = isobmff_syntax::RawBox::new(&tkhd1).unwrap();
        TrackIdUniquenessRule.validate_box(&mut ctx, &raw1);

        let tkhd2 = make_tkhd_v0(1); // Same ID
        let raw2 = isobmff_syntax::RawBox::new(&tkhd2).unwrap();
        TrackIdUniquenessRule.validate_box(&mut ctx, &raw2);

        assert!(ctx.has_errors());
        assert!(ctx.diagnostics()[0].message().contains("Duplicate"));
    }

    #[test]
    fn zero_track_id() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        let tkhd = make_tkhd_v0(0);
        let raw = isobmff_syntax::RawBox::new(&tkhd).unwrap();
        TrackIdUniquenessRule.validate_box(&mut ctx, &raw);

        assert!(ctx.has_errors());
        assert!(ctx.diagnostics()[0].message().contains("0"));
    }

    #[test]
    fn media_track_video() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        let hdlr = make_hdlr(b"vide");
        let raw = isobmff_syntax::RawBox::new(&hdlr).unwrap();
        MediaTrackPresenceRule.validate_box(&mut ctx, &raw);

        assert!(ctx.state().has_media_track);

        MediaTrackPresenceRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn no_media_track() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        let hdlr = make_hdlr(b"hint");
        let raw = isobmff_syntax::RawBox::new(&hdlr).unwrap();
        MediaTrackPresenceRule.validate_box(&mut ctx, &raw);

        assert!(!ctx.state().has_media_track);

        MediaTrackPresenceRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
        assert!(ctx.diagnostics()[0].message().contains("media track"));
    }

    #[test]
    fn t007_valid_version() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let tkhd = make_tkhd_v0(1);
        let raw = isobmff_syntax::RawBox::new(&tkhd).unwrap();
        TkhdVersionRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn t007_invalid_version() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tkhd = make_tkhd_v0(1);
        tkhd[8] = 2; // Set version to 2
        let raw = isobmff_syntax::RawBox::new(&tkhd).unwrap();
        TkhdVersionRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }

    #[test]
    fn t008_valid_flags() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let tkhd = make_tkhd_v0(1); // flags = 0x07
        let raw = isobmff_syntax::RawBox::new(&tkhd).unwrap();
        TkhdFlagsRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn t008_undefined_flags() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tkhd = make_tkhd_v0(1);
        tkhd[9] = 0xFF; // Set undefined flag bits
        let raw = isobmff_syntax::RawBox::new(&tkhd).unwrap();
        TkhdFlagsRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn t009_single_tref() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = crate::context::ContainerChildTracker::default();
        tracker.record_child(BoxCode::TKHD);
        tracker.record_child(BoxCode::MDIA);
        tracker.record_child(BoxCode::TREF);
        ctx.state_mut().container_stack.push((BoxCode::TRAK, tracker));
        TrefUniquenessRule.exit_container(&mut ctx, BoxCode::TRAK);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn t010_no_duplicate_types() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let child1 = make_box(b"hint", &1u32.to_be_bytes());
        let child2 = make_box(b"cdsc", &2u32.to_be_bytes());
        let mut payload = Vec::new();
        payload.extend(&child1);
        payload.extend(&child2);
        let tref = make_box(b"tref", &payload);
        let raw = isobmff_syntax::RawBox::new(&tref).unwrap();
        TrefNoDuplicateTypesRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn t010_duplicate_types() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let child1 = make_box(b"hint", &1u32.to_be_bytes());
        let child2 = make_box(b"hint", &2u32.to_be_bytes());
        let mut payload = Vec::new();
        payload.extend(&child1);
        payload.extend(&child2);
        let tref = make_box(b"tref", &payload);
        let raw = isobmff_syntax::RawBox::new(&tref).unwrap();
        TrefNoDuplicateTypesRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }
}
