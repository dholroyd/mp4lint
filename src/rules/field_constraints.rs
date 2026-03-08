
use crate::context::ValidationContext;
use crate::diagnostic::{BoxPath, DiagnosticType, PropertyPath};
use crate::rules::ValidationRule;
use isobmff_syntax::boxes::{
    HandlerReferenceBoxView,
    MovieHeaderBox, MovieHeaderBoxView,
    TrackHeaderBox, TrackHeaderBoxView,
    VideoMediaHeaderBox, VideoMediaHeaderBoxView,
};
use isobmff_syntax::{BoxCode, FullBoxHeader, HandlerCode, RawBox};

pub struct FullBoxVersionRule;

impl ValidationRule for FullBoxVersionRule {
    fn code(&self) -> &'static str {
        "FC001"
    }

    fn name(&self) -> &'static str {
        "fullbox-version"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let bt = raw_box.box_type();
        let version_01_boxes = [
            BoxCode::MVHD, BoxCode::TKHD, BoxCode::MDHD, BoxCode::HDLR,
            BoxCode::VMHD, BoxCode::SMHD, BoxCode::HMHD, BoxCode::NMHD,
            BoxCode::STHD, BoxCode::STSD, BoxCode::STTS, BoxCode::STSC,
            BoxCode::STSZ, BoxCode::STCO, BoxCode::CO64, BoxCode::STSS,
            BoxCode::CTTS, BoxCode::DREF, BoxCode::ELST, BoxCode::MFHD,
            BoxCode::TFHD, BoxCode::TRUN, BoxCode::TREX, BoxCode::TFDT,
        ];
        if !version_01_boxes.contains(&bt) {
            return;
        }

        let Ok(header) = FullBoxHeader::parse(raw_box.data(), raw_box.data().len()) else {
            return;
        };
        if header.version > 1 {
            ctx.emit_property(DiagnosticType::FullBoxInvalidVersion {
                box_type: bt.0,
                version: header.version,
            }, PropertyPath::from("version"));
        }
    }
}

pub struct ReservedFieldsZeroRule;

impl ValidationRule for ReservedFieldsZeroRule {
    fn code(&self) -> &'static str {
        "FC002"
    }

    fn name(&self) -> &'static str {
        "reserved-fields-zero"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        isobmff_syntax::dispatch_box!(raw_box, {
            TrackHeaderBoxView(tkhd) => self.check_tkhd_reserved(ctx, &tkhd),
            MovieHeaderBoxView(mvhd) => self.check_mvhd_reserved(ctx, &mvhd),
            _ => {},
            Err(_) => {}
        });
    }
}

impl ReservedFieldsZeroRule {
    fn check_tkhd_reserved(&self, ctx: &mut ValidationContext, tkhd: &TrackHeaderBoxView<'_>) {
        if tkhd.reserved_after_track_id() != [0, 0, 0, 0] {
            ctx.emit_property(DiagnosticType::TkhdNonZeroReserved, PropertyPath::from("reserved"));
        }
    }

    fn check_mvhd_reserved(&self, ctx: &mut ValidationContext, mvhd: &MovieHeaderBoxView<'_>) {
        if mvhd.reserved_after_volume() != [0u8; 10] {
            ctx.emit_property(DiagnosticType::MvhdNonZeroReserved, PropertyPath::from("reserved"));
        }
    }
}

pub struct MatrixValuesRule;

impl ValidationRule for MatrixValuesRule {
    fn code(&self) -> &'static str {
        "FC003"
    }

    fn name(&self) -> &'static str {
        "matrix-values"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        isobmff_syntax::dispatch_box!(raw_box, {
            TrackHeaderBoxView(tkhd) => {
                let m = tkhd.matrix();
                let w = m[8];
                if w != 0 && w != 0x40000000u32 as i32 {
                    ctx.emit_property(DiagnosticType::MatrixNonStandard {
                        box_name: "TrackHeaderBox (tkhd)",
                        value: w,
                    }, PropertyPath::from("matrix"));
                }
            },
            MovieHeaderBoxView(mvhd) => {
                let m = mvhd.matrix();
                let w = m[8];
                if w != 0 && w != 0x40000000u32 as i32 {
                    ctx.emit_property(DiagnosticType::MatrixNonStandard {
                        box_name: "MovieHeaderBox (mvhd)",
                        value: w,
                    }, PropertyPath::from("matrix"));
                }
            },
            _ => {},
            Err(_) => {}
        });
    }
}

pub struct VolumeFieldRule;

impl ValidationRule for VolumeFieldRule {
    fn code(&self) -> &'static str {
        "FC004"
    }

    fn name(&self) -> &'static str {
        "volume-field"
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

        let vol = tkhd.volume().raw();
        if vol != 0x0100 && vol != 0x0000 {
            ctx.emit_property(DiagnosticType::VolumeNonStandard { value: vol as u16 }, PropertyPath::from("volume"));
        }
    }
}

pub struct NonVisualTrackDimensionsRule;

impl ValidationRule for NonVisualTrackDimensionsRule {
    fn code(&self) -> &'static str {
        "FC005"
    }

    fn name(&self) -> &'static str {
        "non-visual-track-dimensions"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let warnings: Vec<(u32, HandlerCode, u32, u32, Option<BoxPath>)> = {
            let state = ctx.state();
            state
                .track_info
                .iter()
                .filter_map(|(&track_id, info)| {
                    let ht = info.handler_type?;
                    if (ht == HandlerCode::SOUN || ht == HandlerCode::HINT
                        || ht == HandlerCode::META || ht == HandlerCode::TEXT
                        || ht == HandlerCode::SUBT)
                        && (info.tkhd_width != 0 || info.tkhd_height != 0)
                    {
                        Some((track_id, ht, info.tkhd_width, info.tkhd_height, info.tkhd_path.clone()))
                    } else {
                        None
                    }
                })
                .collect()
        };
        for (track_id, ht, width, height, path) in warnings {
            let prop = if width != 0 { "width" } else { "height" };
            ctx.emit_at_property(DiagnosticType::NonVisualTrackDimensions {
                track_id,
                handler: ht.0,
                width,
                height,
            }, path, PropertyPath::from(prop));
        }
    }
}

pub struct UnusedFlagBitsRule;

impl ValidationRule for UnusedFlagBitsRule {
    fn code(&self) -> &'static str {
        "FC006"
    }

    fn name(&self) -> &'static str {
        "unused-flag-bits"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() == BoxCode::VMHD {
            let Ok(vmhd) = VideoMediaHeaderBoxView::new(raw_box.data()) else {
                return;
            };
            let flags = vmhd.flags();
            if flags & !0x000001 != 0 {
                ctx.emit_property(DiagnosticType::VmhdUndefinedFlags { flags }, PropertyPath::from("flags"));
            }
        }
    }
}

pub struct PreDefinedFieldsZeroRule;

impl ValidationRule for PreDefinedFieldsZeroRule {
    fn code(&self) -> &'static str {
        "FC007"
    }

    fn name(&self) -> &'static str {
        "pre-defined-fields-zero"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::HDLR {
            return;
        }

        let Ok(hdlr) = HandlerReferenceBoxView::new(raw_box.data()) else {
            return;
        };
        if hdlr.pre_defined() != 0 {
            ctx.emit_property(DiagnosticType::HdlrPreDefinedNonZero, PropertyPath::from("pre_defined"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{TrackInfo, ValidationOptions};

    fn make_hdlr_with_predefined(handler_type: &[u8; 4], pre_defined: u32) -> Vec<u8> {
        let mut data = Vec::new();
        let size = 8 + 4 + 4 + 4 + 12 + 1;
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"hdlr");
        data.extend_from_slice(&[0, 0, 0, 0]); // version + flags
        data.extend_from_slice(&pre_defined.to_be_bytes()); // pre_defined
        data.extend_from_slice(handler_type);
        data.extend_from_slice(&[0u8; 12]); // reserved
        data.push(0); // null name
        data
    }

    #[test]
    fn fc001_valid_version() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mvhd = crate::rules::timing::tests::make_mvhd(48000);
        let raw = RawBox::new(&mvhd).unwrap();
        FullBoxVersionRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn fc001_invalid_version() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut mvhd = crate::rules::timing::tests::make_mvhd(48000);
        mvhd[8] = 2;
        let raw = RawBox::new(&mvhd).unwrap();
        FullBoxVersionRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }

    #[test]
    fn fc005_audio_with_dimensions() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut info = TrackInfo::default();
        info.handler_type = Some(HandlerCode::SOUN);
        info.tkhd_width = 1920 << 16;
        info.tkhd_height = 1080 << 16;
        ctx.state_mut().track_info.insert(1, info);
        NonVisualTrackDimensionsRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn fc005_audio_zero_dimensions() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut info = TrackInfo::default();
        info.handler_type = Some(HandlerCode::SOUN);
        info.tkhd_width = 0;
        info.tkhd_height = 0;
        ctx.state_mut().track_info.insert(1, info);
        NonVisualTrackDimensionsRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn fc007_hdlr_pre_defined_zero() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let hdlr = make_hdlr_with_predefined(b"vide", 0);
        let raw = RawBox::new(&hdlr).unwrap();
        PreDefinedFieldsZeroRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn fc007_hdlr_pre_defined_nonzero() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let hdlr = make_hdlr_with_predefined(b"vide", 0x12345678);
        let raw = RawBox::new(&hdlr).unwrap();
        PreDefinedFieldsZeroRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
    }

}
