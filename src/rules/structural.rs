
use crate::context::ValidationContext;
use crate::diagnostic::DiagnosticType;
use crate::rules::ValidationRule;
use isobmff_syntax::{BoxCode, BoxInfo};

pub struct FtypPresenceRule;

impl ValidationRule for FtypPresenceRule {
    fn code(&self) -> &'static str {
        "E001"
    }

    fn name(&self) -> &'static str {
        "ftyp-presence"
    }

    fn observe_box(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
        if info.box_type == BoxCode::FTYP {
            ctx.state_mut().has_ftyp = true;
        }
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        if !ctx.state().has_ftyp {
            ctx.emit(DiagnosticType::FtypMissing);
        }
    }
}

pub struct FtypPositionRule;

impl ValidationRule for FtypPositionRule {
    fn code(&self) -> &'static str {
        "E002"
    }

    fn name(&self) -> &'static str {
        "ftyp-position"
    }

    fn observe_box(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
        let state = ctx.state_mut();
        if info.box_type == BoxCode::FTYP && state.ftyp_offset.is_none() {
            state.ftyp_offset = Some(info.offset);
        } else if info.box_type == BoxCode::MDAT && state.first_mdat_offset.is_none() {
            state.first_mdat_offset = Some(info.offset);
        }
        if info.box_type == BoxCode::MOOV && state.first_moov_offset.is_none() {
            state.first_moov_offset = Some(info.offset);
        }
        if (info.box_type == BoxCode::FREE || info.box_type == BoxCode::SKIP)
            && state.first_free_skip_offset.is_none()
        {
            state.first_free_skip_offset = Some(info.offset);
        }
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let ftyp = ctx.state().ftyp_offset;
        let first_mdat = ctx.state().first_mdat_offset;
        let first_moov = ctx.state().first_moov_offset;
        let first_free_skip = ctx.state().first_free_skip_offset;

        if let (Some(ftyp_off), Some(mdat)) = (ftyp, first_mdat)
            && ftyp_off > mdat {
                ctx.emit(DiagnosticType::FtypAfterMdat);
            }
        if let (Some(ftyp_off), Some(moov)) = (ftyp, first_moov)
            && ftyp_off > moov {
                ctx.emit(DiagnosticType::FtypAfterMoviebox);
            }
        if let (Some(ftyp_off), Some(free_skip)) = (ftyp, first_free_skip)
            && ftyp_off > free_skip {
                ctx.emit(DiagnosticType::FtypAfterFreeSpace);
            }
    }
}

pub struct MoovUniquenessRule;

impl ValidationRule for MoovUniquenessRule {
    fn code(&self) -> &'static str {
        "E003"
    }

    fn name(&self) -> &'static str {
        "moov-uniqueness"
    }

    fn observe_box(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
        if info.box_type == BoxCode::MOOV {
            ctx.state_mut().moov_count += 1;
        }
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let count = ctx.state().moov_count;
        if count == 0 {
            ctx.emit(DiagnosticType::MoovMissing);
        } else if count > 1 {
            ctx.emit(DiagnosticType::MoovDuplicate { count });
        }
    }
}

pub struct BoxHierarchyRule;

impl ValidationRule for BoxHierarchyRule {
    fn code(&self) -> &'static str {
        "E004"
    }

    fn name(&self) -> &'static str {
        "box-hierarchy"
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        let missing: Vec<(BoxCode, BoxCode)> = {
            let Some(tracker) = ctx.current_container_tracker() else {
                return;
            };
            let mut pairs = Vec::new();
            match box_type {
                BoxCode::MOOV => {
                    if !tracker.has_child(BoxCode::MVHD) {
                        pairs.push((BoxCode::MOOV, BoxCode::MVHD));
                    }
                    if !tracker.has_child(BoxCode::TRAK) {
                        pairs.push((BoxCode::MOOV, BoxCode::TRAK));
                    }
                }
                BoxCode::TRAK => {
                    if !tracker.has_child(BoxCode::TKHD) {
                        pairs.push((BoxCode::TRAK, BoxCode::TKHD));
                    }
                    if !tracker.has_child(BoxCode::MDIA) {
                        pairs.push((BoxCode::TRAK, BoxCode::MDIA));
                    }
                }
                BoxCode::MDIA => {
                    if !tracker.has_child(BoxCode::MDHD) {
                        pairs.push((BoxCode::MDIA, BoxCode::MDHD));
                    }
                    if !tracker.has_child(BoxCode::HDLR) {
                        pairs.push((BoxCode::MDIA, BoxCode::HDLR));
                    }
                    if !tracker.has_child(BoxCode::MINF) {
                        pairs.push((BoxCode::MDIA, BoxCode::MINF));
                    }
                }
                BoxCode::MINF => {
                    if !tracker.has_child(BoxCode::STBL) {
                        pairs.push((BoxCode::MINF, BoxCode::STBL));
                    }
                }
                _ => {}
            }
            pairs
        };

        for (parent, child) in missing {
            ctx.emit(DiagnosticType::HierarchyChildMissing { parent, child });
        }
    }
}

pub struct BoxSizeValidityRule;

impl ValidationRule for BoxSizeValidityRule {
    fn code(&self) -> &'static str {
        "F005"
    }

    fn name(&self) -> &'static str {
        "box-size-validity"
    }

    fn observe_box(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
        if let isobmff_syntax::BoxSize::Known(size) = info.size {
            if size.get() < info.header_size as u64 {
                ctx.emit(DiagnosticType::BoxSizeTooSmall {
                    box_type: info.box_type.0,
                    offset: info.offset,
                    size: size.get(),
                    header_size: info.header_size as u32,
                });
            }
        }
    }
}

pub struct BoxTypePrintableAsciiRule;

impl ValidationRule for BoxTypePrintableAsciiRule {
    fn code(&self) -> &'static str {
        "F006"
    }

    fn name(&self) -> &'static str {
        "box-type-printable-ascii"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn observe_box(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
        let bytes = (info.box_type.0).0;
        if !bytes.iter().all(|&b| (0x20..=0x7E).contains(&b)) {
            ctx.emit(DiagnosticType::BoxTypeNonPrintable {
                offset: info.offset,
                bytes,
            });
        }
    }
}

pub struct ReservedBoxTypesRule;

impl ValidationRule for ReservedBoxTypesRule {
    fn code(&self) -> &'static str {
        "F007"
    }

    fn name(&self) -> &'static str {
        "reserved-box-types"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn observe_box(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
        let reserved: &[BoxCode] = &[
            BoxCode::CLIP, BoxCode::CRGN, BoxCode::MATT,
            BoxCode::KMAT, BoxCode::PNOT, BoxCode::CTAB,
            BoxCode::LOAD, BoxCode::IMAP,
        ];
        if reserved.contains(&info.box_type) {
            ctx.emit(DiagnosticType::ReservedBoxType {
                box_type: info.box_type.0,
                offset: info.offset,
            });
        }
    }
}

pub struct TopLevelBoxTypesRule;

impl ValidationRule for TopLevelBoxTypesRule {
    fn code(&self) -> &'static str {
        "F008"
    }

    fn name(&self) -> &'static str {
        "top-level-box-types"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn observe_box(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
        let valid_top_level: &[BoxCode] = &[
            BoxCode::FTYP, BoxCode::MOOV, BoxCode::MDAT, BoxCode::FREE,
            BoxCode::SKIP, BoxCode::META, BoxCode::MOOF, BoxCode::MFRA,
            BoxCode::SIDX, BoxCode::SSIX, BoxCode::PRFT,
            BoxCode::STYP, BoxCode::PDIN, BoxCode::IMDA,
            BoxCode::UUID,
        ];
        if !valid_top_level.contains(&info.box_type) {
            ctx.emit(DiagnosticType::UnexpectedTopLevelBox {
                box_type: info.box_type.0,
                offset: info.offset,
            });
        }
    }
}

pub struct ZeroSizedBoxNotLastRule;

impl ValidationRule for ZeroSizedBoxNotLastRule {
    fn code(&self) -> &'static str {
        "F010"
    }

    fn name(&self) -> &'static str {
        "zero-sized-box-not-last"
    }

    fn observe_child(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
        let stack = &mut ctx.state_mut().zero_sized_stack;
        if let Some(Some((prev_type, prev_offset))) = stack.last().copied() {
            ctx.emit(DiagnosticType::ZeroSizedBoxNotLast {
                box_type: prev_type,
                offset: prev_offset,
            });
        }
        let is_zero = matches!(info.size, isobmff_syntax::BoxSize::ToEnd);
        if let Some(entry) = ctx.state_mut().zero_sized_stack.last_mut() {
            *entry = if is_zero {
                Some((info.box_type.0, info.offset))
            } else {
                None
            };
        }
    }

    fn enter_container(&self, ctx: &mut ValidationContext, _box_type: BoxCode, _info: &BoxInfo) {
        ctx.state_mut().zero_sized_stack.push(None);
    }

    fn exit_container(&self, ctx: &mut ValidationContext, _box_type: BoxCode) {
        ctx.state_mut().zero_sized_stack.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ValidationOptions;

    fn make_box_info(box_type: BoxCode, offset: u64, size: u64) -> BoxInfo {
        BoxInfo {
            box_type,
            offset,
            size: isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(size).unwrap()),
            header_size: 8,
        }
    }

    #[test]
    fn ftyp_presence_missing() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        FtypPresenceRule.observe_box(&mut ctx, &make_box_info(BoxCode::MOOV, 0, 100));

        FtypPresenceRule.finalize(&mut ctx);

        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "E001");
    }

    #[test]
    fn ftyp_presence_found() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        FtypPresenceRule.observe_box(&mut ctx, &make_box_info(BoxCode::FTYP, 0, 20));

        FtypPresenceRule.finalize(&mut ctx);

        assert!(!ctx.has_errors());
    }

    #[test]
    fn ftyp_position_after_mdat() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        FtypPositionRule.observe_box(&mut ctx, &make_box_info(BoxCode::MDAT, 0, 1000));
        FtypPositionRule.observe_box(&mut ctx, &make_box_info(BoxCode::FTYP, 1000, 20));

        FtypPositionRule.finalize(&mut ctx);

        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "E002");
    }

    #[test]
    fn ftyp_position_not_first() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        FtypPositionRule.observe_box(&mut ctx, &make_box_info(BoxCode::MDAT, 0, 1000));
        FtypPositionRule.observe_box(&mut ctx, &make_box_info(BoxCode::MOOV, 1000, 100));
        FtypPositionRule.observe_box(&mut ctx, &make_box_info(BoxCode::FREE, 1100, 10));
        FtypPositionRule.observe_box(&mut ctx, &make_box_info(BoxCode::FTYP, 1110, 20));

        FtypPositionRule.finalize(&mut ctx);

        assert!(ctx.diagnostics().iter().any(|d|
            d.diagnostic_type == DiagnosticType::FtypAfterMdat));
        assert!(ctx.diagnostics().iter().any(|d|
            d.diagnostic_type == DiagnosticType::FtypAfterMoviebox));
        assert!(ctx.diagnostics().iter().any(|d|
            d.diagnostic_type == DiagnosticType::FtypAfterFreeSpace));
    }

    #[test]
    fn ftyp_position_before_all() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        FtypPositionRule.observe_box(&mut ctx, &make_box_info(BoxCode::FTYP, 0, 20));
        FtypPositionRule.observe_box(&mut ctx, &make_box_info(BoxCode::FREE, 20, 10));
        FtypPositionRule.observe_box(&mut ctx, &make_box_info(BoxCode::MOOV, 30, 100));
        FtypPositionRule.observe_box(&mut ctx, &make_box_info(BoxCode::MDAT, 130, 1000));

        FtypPositionRule.finalize(&mut ctx);

        assert!(!ctx.has_errors());
    }

    #[test]
    fn moov_missing() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        MoovUniquenessRule.observe_box(&mut ctx, &make_box_info(BoxCode::FTYP, 0, 20));

        MoovUniquenessRule.finalize(&mut ctx);

        assert!(ctx.has_errors());
        assert!(ctx.diagnostics()[0].message().contains("missing"));
    }

    #[test]
    fn moov_duplicate() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        MoovUniquenessRule.observe_box(&mut ctx, &make_box_info(BoxCode::MOOV, 0, 100));
        MoovUniquenessRule.observe_box(&mut ctx, &make_box_info(BoxCode::MOOV, 100, 100));

        MoovUniquenessRule.finalize(&mut ctx);

        assert!(ctx.has_errors());
        assert!(ctx.diagnostics()[0].message().contains("2"));
    }

    #[test]
    fn moov_single() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        MoovUniquenessRule.observe_box(&mut ctx, &make_box_info(BoxCode::MOOV, 0, 100));

        MoovUniquenessRule.finalize(&mut ctx);

        assert!(!ctx.has_errors());
    }

    #[test]
    fn f005_valid_size() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        BoxSizeValidityRule.observe_box(&mut ctx, &make_box_info(BoxCode::FTYP, 0, 20));
        assert!(!ctx.has_errors());
    }

    #[test]
    fn f006_printable_ascii() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        BoxTypePrintableAsciiRule.observe_box(&mut ctx, &make_box_info(BoxCode::FTYP, 0, 20));
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn f006_non_printable() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let info = make_box_info(BoxCode::new(*b"\x00\x01\x02\x03"), 0, 8);
        BoxTypePrintableAsciiRule.observe_box(&mut ctx, &info);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn f007_reserved_type() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let info = make_box_info(BoxCode::CLIP, 0, 8);
        ReservedBoxTypesRule.observe_box(&mut ctx, &info);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn f007_non_reserved_type() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ReservedBoxTypesRule.observe_box(&mut ctx, &make_box_info(BoxCode::FTYP, 0, 20));
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn f008_valid_top_level() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        TopLevelBoxTypesRule.observe_box(&mut ctx, &make_box_info(BoxCode::FTYP, 0, 20));
        TopLevelBoxTypesRule.observe_box(&mut ctx, &make_box_info(BoxCode::MOOV, 20, 100));
        TopLevelBoxTypesRule.observe_box(&mut ctx, &make_box_info(BoxCode::MDAT, 120, 1000));
        assert_eq!(ctx.warning_count(), 0);
    }

    fn make_box_info_to_end(box_type: BoxCode, offset: u64) -> BoxInfo {
        BoxInfo {
            box_type,
            offset,
            size: isobmff_syntax::BoxSize::ToEnd,
            header_size: 8,
        }
    }

    #[test]
    fn f010_zero_sized_last_top_level_no_error() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info(BoxCode::FTYP, 0, 20));
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info_to_end(BoxCode::MDAT, 20));
        assert!(!ctx.has_errors());
    }

    #[test]
    fn f010_zero_sized_not_last_top_level() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info_to_end(BoxCode::MDAT, 0));
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info(BoxCode::MOOV, 100, 50));
        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "F010");
        assert!(ctx.diagnostics()[0].message().contains("mdat"));
    }

    #[test]
    fn f010_zero_sized_last_in_container_no_error() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ZeroSizedBoxNotLastRule.enter_container(&mut ctx, BoxCode::MOOV, &make_box_info(BoxCode::MOOV, 0, 100));
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info(BoxCode::MVHD, 8, 20));
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info_to_end(BoxCode::TRAK, 28));
        ZeroSizedBoxNotLastRule.exit_container(&mut ctx, BoxCode::MOOV);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn f010_zero_sized_not_last_in_container() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ZeroSizedBoxNotLastRule.enter_container(&mut ctx, BoxCode::MOOV, &make_box_info(BoxCode::MOOV, 0, 100));
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info_to_end(BoxCode::MVHD, 8));
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info(BoxCode::TRAK, 28, 50));
        ZeroSizedBoxNotLastRule.exit_container(&mut ctx, BoxCode::MOOV);
        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "F010");
        assert!(ctx.diagnostics()[0].message().contains("mvhd"));
    }

    #[test]
    fn f010_nested_containers_independent() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info(BoxCode::FTYP, 0, 20));
        ZeroSizedBoxNotLastRule.enter_container(&mut ctx, BoxCode::MOOV, &make_box_info(BoxCode::MOOV, 20, 100));
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info_to_end(BoxCode::TRAK, 28));
        ZeroSizedBoxNotLastRule.exit_container(&mut ctx, BoxCode::MOOV);
        ZeroSizedBoxNotLastRule.observe_child(&mut ctx, &make_box_info(BoxCode::MDAT, 120, 500));
        assert!(!ctx.has_errors());
    }

    #[test]
    fn f008_invalid_top_level() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let info = make_box_info(BoxCode::TRAK, 0, 100);
        TopLevelBoxTypesRule.observe_box(&mut ctx, &info);
        assert_eq!(ctx.warning_count(), 1);
    }

}
