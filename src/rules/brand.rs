
use crate::context::ValidationContext;
use crate::diagnostic::DiagnosticType;
use crate::rules::ValidationRule;
use isobmff_syntax::{BoxCode, BoxInfo, BrandCode, FullBoxHeader, RawBox};

fn brand_level(brand: BrandCode) -> Option<u8> {
    match brand {
        BrandCode::ISOM => Some(0),
        BrandCode::AVC1 => Some(1),
        BrandCode::ISO2 => Some(2),
        BrandCode::ISO3 => Some(3),
        BrandCode::ISO4 => Some(4),
        BrandCode::ISO5 => Some(5),
        BrandCode::ISO6 => Some(6),
        BrandCode::ISO7 => Some(7),
        BrandCode::ISO8 => Some(8),
        BrandCode::ISO9 => Some(9),
        BrandCode::ISOA => Some(10),
        BrandCode::ISOB => Some(11),
        BrandCode::ISOC => Some(12),
        _ => None,
    }
}

fn has_brand_at_least(brands: &[BrandCode], minimum: BrandCode) -> bool {
    let Some(min_level) = brand_level(minimum) else {
        return false;
    };
    brands
        .iter()
        .filter_map(|b| brand_level(*b))
        .any(|level| level >= min_level)
}

pub struct BrandComplianceRule;

impl ValidationRule for BrandComplianceRule {
    fn code(&self) -> &'static str {
        "BR001"
    }

    fn name(&self) -> &'static str {
        "brand-compliance"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn enter_container(&self, ctx: &mut ValidationContext, box_type: BoxCode, _info: &BoxInfo) {
        match box_type {
            BoxCode::META => {
                let in_moof = ctx
                    .state()
                    .container_stack
                    .iter()
                    .any(|(bt, _)| *bt == BoxCode::MOOF);
                if in_moof {
                    ctx.state_mut().brand_features.seen_meta_in_moof = true;
                } else {
                    ctx.state_mut().brand_features.seen_meta = true;
                }
            }
            BoxCode::IPRP => {
                ctx.state_mut().brand_features.seen_iprp = true;
            }
            BoxCode::GRPL => {
                ctx.state_mut().brand_features.seen_grpl = true;
            }
            _ => {}
        }
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let bt = raw_box.box_type();
        match bt {
            BoxCode::SDTP => {
                ctx.state_mut().brand_features.seen_sdtp = true;
            }
            BoxCode::SBGP => {
                ctx.state_mut().brand_features.seen_sbgp = true;
            }
            BoxCode::SGPD => {
                let in_traf = ctx
                    .state()
                    .container_stack
                    .iter()
                    .any(|(bt, _)| *bt == BoxCode::TRAF);
                if in_traf {
                    ctx.state_mut().brand_features.seen_sgpd_in_traf = true;
                } else {
                    ctx.state_mut().brand_features.seen_sgpd = true;
                }
            }
            BoxCode::SUBS => {
                ctx.state_mut().brand_features.seen_subs = true;
            }
            BoxCode::PDIN => {
                ctx.state_mut().brand_features.seen_pdin = true;
            }
            BoxCode::FIIN => {
                ctx.state_mut().brand_features.seen_fiin = true;
            }
            BoxCode::TRGR => {
                ctx.state_mut().brand_features.seen_trgr = true;
            }
            BoxCode::IDAT => {
                ctx.state_mut().brand_features.seen_idat = true;
            }
            BoxCode::IREF => {
                ctx.state_mut().brand_features.seen_iref = true;
            }
            BoxCode::TFDT => {
                ctx.state_mut().brand_features.seen_tfdt = true;
            }
            BoxCode::TREP => {
                ctx.state_mut().brand_features.seen_trep = true;
            }
            BoxCode::STHD => {
                ctx.state_mut().brand_features.seen_sthd = true;
            }
            BoxCode::ELNG => {
                ctx.state_mut().brand_features.seen_elng = true;
            }
            BoxCode::SIDX => {
                ctx.state_mut().brand_features.seen_sidx = true;
            }
            BoxCode::SSIX => {
                ctx.state_mut().brand_features.seen_ssix = true;
            }
            BoxCode::STYP => {
                ctx.state_mut().brand_features.seen_styp = true;
            }
            BoxCode::PRFT => {
                ctx.state_mut().brand_features.seen_prft = true;
            }
            BoxCode::CTTS => {
                if let Ok(h) = FullBoxHeader::parse(raw_box.data(), raw_box.data().len())
                    && h.version == 1
                {
                    ctx.state_mut().brand_features.seen_ctts_v1 = true;
                }
            }
            BoxCode::TRUN => {
                if let Ok(h) = FullBoxHeader::parse(raw_box.data(), raw_box.data().len())
                    && h.version == 1
                {
                    ctx.state_mut().brand_features.seen_trun_v1 = true;
                }
            }
            BoxCode::CSLG => {
                if let Ok(h) = FullBoxHeader::parse(raw_box.data(), raw_box.data().len()) {
                    if h.version == 1 {
                        ctx.state_mut().brand_features.seen_cslg_v1 = true;
                    } else {
                        ctx.state_mut().brand_features.seen_cslg_v0 = true;
                    }
                }
            }
            _ => {}
        }
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let brands = ctx.state().compatible_brands.clone();
        let features = &ctx.state().brand_features;

        let checks: &[(bool, u8, BrandCode, &str)] = &[
            (features.seen_sdtp, 1, BrandCode::AVC1,
             "SampleDependencyTypeBox (sdtp)"),
            (features.seen_sbgp, 2, BrandCode::AVC1,
             "SampleToGroupBox (sbgp)"),
            (features.seen_sgpd, 3, BrandCode::AVC1,
             "SampleGroupDescriptionBox (sgpd)"),
            (features.seen_meta, 4, BrandCode::ISO2,
             "MetaBox (meta)"),
            (features.seen_pdin, 5, BrandCode::ISO2,
             "ProgressiveDownloadInfoBox (pdin)"),
            (features.seen_subs, 6, BrandCode::ISO2,
             "SubSampleInformationBox (subs)"),
            (features.seen_fiin, 7, BrandCode::ISO3,
             "FDItemInformationBox (fiin)"),
            (features.seen_ctts_v1, 8, BrandCode::ISO4,
             "CompositionOffsetBox (ctts) version 1"),
            (features.seen_cslg_v0, 9, BrandCode::ISO4,
             "CompositionToDecodeBox (cslg) version 0"),
            (features.seen_trgr, 10, BrandCode::ISO4,
             "TrackGroupBox (trgr)"),
            (features.seen_idat, 11, BrandCode::ISO4,
             "ItemDataBox (idat)"),
            (features.seen_iref, 12, BrandCode::ISO4,
             "ItemReferenceBox (iref)"),
            (features.seen_tfdt, 13, BrandCode::ISO6,
             "TrackFragmentBaseMediaDecodeTimeBox (tfdt)"),
            (features.seen_trun_v1, 14, BrandCode::ISO6,
             "TrackRunBox (trun) version 1"),
            (features.seen_sidx, 15, BrandCode::ISO6,
             "SegmentIndexBox (sidx)"),
            (features.seen_ssix, 16, BrandCode::ISO6,
             "SubsegmentIndexBox (ssix)"),
            (features.seen_styp, 17, BrandCode::ISO6,
             "SegmentTypeBox (styp)"),
            (features.seen_prft, 18, BrandCode::ISO6,
             "ProducerReferenceTimeBox (prft)"),
            (features.seen_sgpd_in_traf, 19, BrandCode::ISO6,
             "SampleGroupDescriptionBox (sgpd) in traf"),
            (features.seen_trep, 20, BrandCode::ISO7,
             "TrackExtensionPropertiesBox (trep)"),
            (features.seen_sthd, 21, BrandCode::ISO8,
             "SubtitleMediaHeaderBox (sthd)"),
            (features.seen_meta_in_moof, 22, BrandCode::ISO8,
             "MetaBox (meta) in moof"),
            (features.seen_elng, 23, BrandCode::ISO9,
             "ExtendedLanguageBox (elng)"),
            (features.seen_cslg_v1, 24, BrandCode::ISO9,
             "CompositionToDecodeBox (cslg) version 1"),
            (features.seen_iprp, 25, BrandCode::ISOA,
             "ItemPropertiesBox (iprp)"),
            (features.seen_grpl, 26, BrandCode::ISOA,
             "EntityToGroupBox (grpl)"),
        ];

        for &(flag, code_suffix, min_brand, box_name) in checks {
            if flag && !has_brand_at_least(&brands, min_brand) {
                ctx.emit(DiagnosticType::BrandRequired {
                    code_suffix,
                    box_name,
                    min_brand,
                });
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


    #[test]
    fn brand_at_least_exact_match() {
        assert!(has_brand_at_least(&[BrandCode::ISO4], BrandCode::ISO4));
    }

    #[test]
    fn brand_at_least_higher_satisfies() {
        assert!(has_brand_at_least(&[BrandCode::ISO9], BrandCode::ISO4));
    }

    #[test]
    fn brand_at_least_lower_fails() {
        assert!(!has_brand_at_least(&[BrandCode::ISO2], BrandCode::ISO4));
    }

    #[test]
    fn brand_at_least_empty_brands() {
        assert!(!has_brand_at_least(&[], BrandCode::ISOM));
    }

    #[test]
    fn brand_at_least_unknown_brand_ignored() {
        assert!(!has_brand_at_least(&[BrandCode::MP41], BrandCode::ISOM));
    }

    #[test]
    fn brand_at_least_mixed_brands() {
        assert!(has_brand_at_least(
            &[BrandCode::MP41, BrandCode::ISO6],
            BrandCode::ISO4
        ));
    }

    #[test]
    fn brand_at_least_avc1_satisfies_avc1() {
        assert!(has_brand_at_least(&[BrandCode::AVC1], BrandCode::AVC1));
    }

    #[test]
    fn brand_at_least_isom_does_not_satisfy_avc1() {
        assert!(!has_brand_at_least(&[BrandCode::ISOM], BrandCode::AVC1));
    }


    #[test]
    fn detect_sdtp() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let data = make_box(b"sdtp", &[0; 8]);
        let raw = RawBox::new(&data).unwrap();
        BrandComplianceRule.validate_box(&mut ctx, &raw);
        assert!(ctx.state().brand_features.seen_sdtp);
    }

    #[test]
    fn detect_ctts_v1() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let data = make_box(b"ctts", &[1, 0, 0, 0, 0, 0, 0, 0]);
        let raw = RawBox::new(&data).unwrap();
        BrandComplianceRule.validate_box(&mut ctx, &raw);
        assert!(ctx.state().brand_features.seen_ctts_v1);
    }

    #[test]
    fn detect_trun_v1() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let data = make_box(b"trun", &[1, 0, 0, 0, 0, 0, 0, 0]);
        let raw = RawBox::new(&data).unwrap();
        BrandComplianceRule.validate_box(&mut ctx, &raw);
        assert!(ctx.state().brand_features.seen_trun_v1);
    }

    #[test]
    fn detect_cslg_v0() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let data = make_box(b"cslg", &[0, 0, 0, 0, 0, 0, 0, 0]);
        let raw = RawBox::new(&data).unwrap();
        BrandComplianceRule.validate_box(&mut ctx, &raw);
        assert!(ctx.state().brand_features.seen_cslg_v0);
        assert!(!ctx.state().brand_features.seen_cslg_v1);
    }

    #[test]
    fn detect_cslg_v1() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let data = make_box(b"cslg", &[1, 0, 0, 0, 0, 0, 0, 0]);
        let raw = RawBox::new(&data).unwrap();
        BrandComplianceRule.validate_box(&mut ctx, &raw);
        assert!(ctx.state().brand_features.seen_cslg_v1);
        assert!(!ctx.state().brand_features.seen_cslg_v0);
    }

    #[test]
    fn detect_sgpd_in_stbl() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let tracker = crate::context::ContainerChildTracker::default();
        ctx.state_mut()
            .container_stack
            .push((BoxCode::STBL, tracker));
        let data = make_box(b"sgpd", &[0; 12]);
        let raw = RawBox::new(&data).unwrap();
        BrandComplianceRule.validate_box(&mut ctx, &raw);
        assert!(ctx.state().brand_features.seen_sgpd);
        assert!(!ctx.state().brand_features.seen_sgpd_in_traf);
    }

    #[test]
    fn detect_sgpd_in_traf() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let tracker = crate::context::ContainerChildTracker::default();
        ctx.state_mut()
            .container_stack
            .push((BoxCode::TRAF, tracker));
        let data = make_box(b"sgpd", &[0; 12]);
        let raw = RawBox::new(&data).unwrap();
        BrandComplianceRule.validate_box(&mut ctx, &raw);
        assert!(ctx.state().brand_features.seen_sgpd_in_traf);
        assert!(!ctx.state().brand_features.seen_sgpd);
    }

    #[test]
    fn detect_meta_outside_moof() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let info = BoxInfo {
            box_type: BoxCode::META,
            offset: 0,
            size: isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(12).unwrap()),
            header_size: 8,
        };
        BrandComplianceRule.enter_container(&mut ctx, BoxCode::META, &info);
        assert!(ctx.state().brand_features.seen_meta);
        assert!(!ctx.state().brand_features.seen_meta_in_moof);
    }

    #[test]
    fn detect_meta_in_moof() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let tracker = crate::context::ContainerChildTracker::default();
        ctx.state_mut()
            .container_stack
            .push((BoxCode::MOOF, tracker));
        let info = BoxInfo {
            box_type: BoxCode::META,
            offset: 0,
            size: isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(12).unwrap()),
            header_size: 8,
        };
        BrandComplianceRule.enter_container(&mut ctx, BoxCode::META, &info);
        assert!(ctx.state().brand_features.seen_meta_in_moof);
        assert!(!ctx.state().brand_features.seen_meta);
    }

    #[test]
    fn detect_iprp() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let info = BoxInfo {
            box_type: BoxCode::IPRP,
            offset: 0,
            size: isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(8).unwrap()),
            header_size: 8,
        };
        BrandComplianceRule.enter_container(&mut ctx, BoxCode::IPRP, &info);
        assert!(ctx.state().brand_features.seen_iprp);
    }

    #[test]
    fn detect_grpl() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let info = BoxInfo {
            box_type: BoxCode::GRPL,
            offset: 0,
            size: isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(8).unwrap()),
            header_size: 8,
        };
        BrandComplianceRule.enter_container(&mut ctx, BoxCode::GRPL, &info);
        assert!(ctx.state().brand_features.seen_grpl);
    }


    #[test]
    fn finalize_sdtp_without_avc1_warns() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().compatible_brands = vec![BrandCode::ISOM];
        ctx.state_mut().brand_features.seen_sdtp = true;
        BrandComplianceRule.finalize(&mut ctx);
        let warnings: Vec<_> = ctx
            .diagnostics()
            .iter()
            .filter(|d| d.code() == "BR001")
            .collect();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn finalize_sdtp_with_avc1_no_warning() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().compatible_brands = vec![BrandCode::AVC1];
        ctx.state_mut().brand_features.seen_sdtp = true;
        BrandComplianceRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn finalize_feature_satisfied_by_higher_brand() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().compatible_brands = vec![BrandCode::ISO9];
        ctx.state_mut().brand_features.seen_ctts_v1 = true;
        BrandComplianceRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn finalize_feature_not_satisfied_by_lower_brand() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().compatible_brands = vec![BrandCode::ISO4];
        ctx.state_mut().brand_features.seen_tfdt = true;
        BrandComplianceRule.finalize(&mut ctx);
        let warnings: Vec<_> = ctx
            .diagnostics()
            .iter()
            .filter(|d| d.code() == "BR013")
            .collect();
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn finalize_no_features_no_warnings() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().compatible_brands = vec![BrandCode::ISOM];
        BrandComplianceRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn finalize_multiple_features_multiple_warnings() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().compatible_brands = vec![BrandCode::ISOM];
        ctx.state_mut().brand_features.seen_sdtp = true;
        ctx.state_mut().brand_features.seen_meta = true;
        ctx.state_mut().brand_features.seen_tfdt = true;
        BrandComplianceRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 3);
    }

    #[test]
    fn finalize_isoa_features() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().compatible_brands = vec![BrandCode::ISOA];
        ctx.state_mut().brand_features.seen_iprp = true;
        ctx.state_mut().brand_features.seen_grpl = true;
        BrandComplianceRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn finalize_isoa_features_without_brand() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().compatible_brands = vec![BrandCode::ISO9];
        ctx.state_mut().brand_features.seen_iprp = true;
        ctx.state_mut().brand_features.seen_grpl = true;
        BrandComplianceRule.finalize(&mut ctx);
        assert_eq!(ctx.warning_count(), 2);
    }
}
