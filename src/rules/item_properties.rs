
use crate::context::ValidationContext;
use crate::diagnostic::{DiagnosticType, PropertyPath};
use crate::rules::ValidationRule;
use isobmff_syntax::boxes::{
    ItemInfoBoxView,
    ItemInfoEntryBox, ItemInfoEntryBoxView,
    ItemPropertyAssociation, ItemPropertyAssociationBox, ItemPropertyAssociationBoxView,
};
use isobmff_syntax::{BoxCode, RawBox};
use std::collections::HashSet;

pub struct IinfEntryOrderingRule;

impl ValidationRule for IinfEntryOrderingRule {
    fn code(&self) -> &'static str {
        "IP001"
    }

    fn name(&self) -> &'static str {
        "iinf-entry-ordering"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::IINF {
            return;
        }

        let Ok(iinf) = ItemInfoBoxView::new(raw_box.data()) else {
            return;
        };

        let mut prev_id: Option<u32> = None;
        for infe_raw in iinf.infe_boxes() {
            let Ok(infe) = ItemInfoEntryBoxView::new(infe_raw.data()) else {
                continue;
            };
            let id = infe.item_id();
            if let Some(prev) = prev_id {
                if id <= prev {
                    ctx.emit(DiagnosticType::IinfEntryOrderNotIncreasing {
                        prev_id: prev,
                        curr_id: id,
                    });
                    return;
                }
            }
            prev_id = Some(id);
        }
    }
}

pub struct IpmaItemIdOrderingRule;

impl ValidationRule for IpmaItemIdOrderingRule {
    fn code(&self) -> &'static str {
        "IP002"
    }

    fn name(&self) -> &'static str {
        "ipma-item-id-ordering"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::IPMA {
            return;
        }

        let Ok(ipma) = ItemPropertyAssociationBoxView::new(raw_box.data()) else {
            return;
        };

        let mut prev_id: Option<u32> = None;
        for (i, entry) in ipma.entries().enumerate() {
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    ctx.emit_property(
                        DiagnosticType::BoxParseFailed { detail: err.to_string() },
                        PropertyPath::from_indexed("entry", i),
                    );
                    return;
                }
            };
            if let Some(prev) = prev_id {
                if entry.item_id() <= prev {
                    ctx.emit(DiagnosticType::IpmaItemIdNotIncreasing {
                        prev_id: prev,
                        curr_id: entry.item_id(),
                    });
                    return;
                }
            }
            prev_id = Some(entry.item_id());
        }
    }
}

pub struct IpmaItemIdUniquenessRule;

impl ValidationRule for IpmaItemIdUniquenessRule {
    fn code(&self) -> &'static str {
        "IP003"
    }

    fn name(&self) -> &'static str {
        "ipma-item-id-uniqueness"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::IPMA {
            return;
        }

        let Ok(ipma) = ItemPropertyAssociationBoxView::new(raw_box.data()) else {
            return;
        };

        let path = ctx.path().clone();
        for (i, entry) in ipma.entries().enumerate() {
            let entry = match entry {
                Ok(e) => e,
                Err(err) => {
                    ctx.emit_property(
                        DiagnosticType::BoxParseFailed { detail: err.to_string() },
                        PropertyPath::from_indexed("entry", i),
                    );
                    return;
                }
            };
            ctx.state_mut().ipma_item_ids.push((entry.item_id(), path.clone()));
        }
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let mut seen = HashSet::new();
        let duplicates: Vec<(u32, _)> = ctx.state().ipma_item_ids.iter()
            .filter(|(id, _)| !seen.insert(*id))
            .cloned()
            .collect();

        for (item_id, path) in duplicates {
            ctx.emit_at(
                DiagnosticType::IpmaItemIdDuplicate { item_id },
                Some(path),
            );
        }
    }
}

pub struct IpmaVersionFlagsUniquenessRule;

impl ValidationRule for IpmaVersionFlagsUniquenessRule {
    fn code(&self) -> &'static str {
        "IP004"
    }

    fn name(&self) -> &'static str {
        "ipma-version-flags-uniqueness"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::IPMA {
            return;
        }

        let Ok(ipma) = ItemPropertyAssociationBoxView::new(raw_box.data()) else {
            return;
        };

        let path = ctx.path().clone();
        ctx.state_mut().ipma_version_flags.push((
            ipma.version(),
            ipma.flags(),
            path,
        ));
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let mut seen = HashSet::new();
        let duplicates: Vec<(u8, u32, _)> = ctx.state().ipma_version_flags.iter()
            .filter(|(v, f, _)| !seen.insert((*v, *f)))
            .cloned()
            .collect();

        for (version, flags, path) in duplicates {
            ctx.emit_at(
                DiagnosticType::IpmaVersionFlagsDuplicate { version, flags },
                Some(path),
            );
        }
    }
}

pub struct IpmaVersionRecommendationRule;

impl ValidationRule for IpmaVersionRecommendationRule {
    fn code(&self) -> &'static str {
        "IP005"
    }

    fn name(&self) -> &'static str {
        "ipma-version-recommendation"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::IPMA {
            return;
        }

        let Ok(ipma) = ItemPropertyAssociationBoxView::new(raw_box.data()) else {
            return;
        };

        if ipma.version() > 0 {
            let needs_large_ids = ipma.entries()
                .filter_map(|e| e.ok())
                .any(|e| e.item_id() > u16::MAX as u32);
            if !needs_large_ids {
                ctx.emit_property(DiagnosticType::IpmaVersionUnnecessary {
                    version: ipma.version(),
                }, PropertyPath::from("version"));
            }
        }
    }
}

pub struct IpmaFlagsRecommendationRule;

impl ValidationRule for IpmaFlagsRecommendationRule {
    fn code(&self) -> &'static str {
        "IP006"
    }

    fn name(&self) -> &'static str {
        "ipma-flags-recommendation"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::IPCO {
            return;
        }
        let count = ctx.current_container_tracker()
            .map(|t| t.children.len() as u32)
            .unwrap_or(0);
        ctx.state_mut().ipco_property_count = Some(count);
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::IPMA {
            return;
        }

        let Ok(ipma) = ItemPropertyAssociationBoxView::new(raw_box.data()) else {
            return;
        };

        if ipma.flags() & 1 != 0 {
            let property_count = ctx.state().ipco_property_count.unwrap_or(0);
            if property_count <= 127 {
                ctx.emit_property(DiagnosticType::IpmaFlagsUnnecessary, PropertyPath::from("flags"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{ContainerChildTracker, ValidationOptions};

    fn make_iinf(item_ids: &[u16]) -> Vec<u8> {
        let mut children = Vec::new();
        for &id in item_ids {
            let infe_size: u32 = 21;
            children.extend_from_slice(&infe_size.to_be_bytes());
            children.extend_from_slice(b"infe");
            children.push(2); // version = 2
            children.extend_from_slice(&[0, 0, 0]); // flags
            children.extend_from_slice(&id.to_be_bytes()); // item_id (v2 uses u16)
            children.extend_from_slice(&0u16.to_be_bytes()); // item_protection_index
            children.extend_from_slice(b"mime"); // item_type
            children.push(0); // null-terminated name
        }

        let total_size = 8 + 4 + 2 + children.len();
        let mut data = Vec::new();
        data.extend_from_slice(&(total_size as u32).to_be_bytes());
        data.extend_from_slice(b"iinf");
        data.push(0); // version = 0
        data.extend_from_slice(&[0, 0, 0]); // flags
        data.extend_from_slice(&(item_ids.len() as u16).to_be_bytes()); // entry_count
        data.extend_from_slice(&children);
        data
    }

    fn make_ipma_v0(item_ids: &[u16]) -> Vec<u8> {
        let entries_size = item_ids.len() * 3;
        let total_size = 8 + 4 + 4 + entries_size;
        let mut data = Vec::new();
        data.extend_from_slice(&(total_size as u32).to_be_bytes());
        data.extend_from_slice(b"ipma");
        data.push(0); // version = 0
        data.extend_from_slice(&[0, 0, 0]); // flags = 0
        data.extend_from_slice(&(item_ids.len() as u32).to_be_bytes());
        for &id in item_ids {
            data.extend_from_slice(&id.to_be_bytes());
            data.push(0); // no associations
        }
        data
    }

    fn make_ipma(version: u8, flags: u32, item_ids: &[u32]) -> Vec<u8> {
        let item_id_size = if version < 1 { 2 } else { 4 };
        let entries_size = item_ids.len() * (item_id_size + 1);
        let total_size = 8 + 4 + 4 + entries_size;
        let mut data = Vec::new();
        data.extend_from_slice(&(total_size as u32).to_be_bytes());
        data.extend_from_slice(b"ipma");
        data.push(version);
        let flags_bytes = flags.to_be_bytes();
        data.extend_from_slice(&flags_bytes[1..4]); // 3 bytes of flags
        data.extend_from_slice(&(item_ids.len() as u32).to_be_bytes());
        for &id in item_ids {
            if version < 1 {
                data.extend_from_slice(&(id as u16).to_be_bytes());
            } else {
                data.extend_from_slice(&id.to_be_bytes());
            }
            data.push(0); // no associations
        }
        data
    }

    #[test]
    fn ip001_sorted_iinf() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let iinf = make_iinf(&[1, 2, 3]);
        let raw = RawBox::new(&iinf).unwrap();
        IinfEntryOrderingRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn ip001_unsorted_iinf() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let iinf = make_iinf(&[1, 3, 2]);
        let raw = RawBox::new(&iinf).unwrap();
        IinfEntryOrderingRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
        assert_eq!(ctx.diagnostics()[0].code(), "IP001");
    }

    #[test]
    fn ip001_duplicate_ids_in_iinf() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let iinf = make_iinf(&[1, 1, 2]);
        let raw = RawBox::new(&iinf).unwrap();
        IinfEntryOrderingRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn ip002_sorted_ipma() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let ipma = make_ipma_v0(&[1, 2, 3]);
        let raw = RawBox::new(&ipma).unwrap();
        IpmaItemIdOrderingRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn ip002_unsorted_ipma() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let ipma = make_ipma_v0(&[2, 1, 3]);
        let raw = RawBox::new(&ipma).unwrap();
        IpmaItemIdOrderingRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "IP002");
    }

    #[test]
    fn ip003_unique_across_ipma_boxes() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        let ipma1 = make_ipma_v0(&[1, 2]);
        let raw1 = RawBox::new(&ipma1).unwrap();
        IpmaItemIdUniquenessRule.validate_box(&mut ctx, &raw1);

        let ipma2 = make_ipma_v0(&[3, 4]);
        let raw2 = RawBox::new(&ipma2).unwrap();
        IpmaItemIdUniquenessRule.validate_box(&mut ctx, &raw2);

        IpmaItemIdUniquenessRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn ip003_duplicate_across_ipma_boxes() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        let ipma1 = make_ipma_v0(&[1, 2]);
        let raw1 = RawBox::new(&ipma1).unwrap();
        IpmaItemIdUniquenessRule.validate_box(&mut ctx, &raw1);

        let ipma2 = make_ipma_v0(&[2, 3]);
        let raw2 = RawBox::new(&ipma2).unwrap();
        IpmaItemIdUniquenessRule.validate_box(&mut ctx, &raw2);

        IpmaItemIdUniquenessRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "IP003");
    }

    #[test]
    fn ip004_unique_version_flags() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        let ipma1 = make_ipma(0, 0, &[1]);
        let raw1 = RawBox::new(&ipma1).unwrap();
        IpmaVersionFlagsUniquenessRule.validate_box(&mut ctx, &raw1);

        let ipma2 = make_ipma(0, 1, &[2]);
        let raw2 = RawBox::new(&ipma2).unwrap();
        IpmaVersionFlagsUniquenessRule.validate_box(&mut ctx, &raw2);

        IpmaVersionFlagsUniquenessRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn ip004_duplicate_version_flags() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        let ipma1 = make_ipma(0, 0, &[1]);
        let raw1 = RawBox::new(&ipma1).unwrap();
        IpmaVersionFlagsUniquenessRule.validate_box(&mut ctx, &raw1);

        let ipma2 = make_ipma(0, 0, &[2]);
        let raw2 = RawBox::new(&ipma2).unwrap();
        IpmaVersionFlagsUniquenessRule.validate_box(&mut ctx, &raw2);

        IpmaVersionFlagsUniquenessRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "IP004");
    }

    #[test]
    fn ip005_v0_no_warning() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let ipma = make_ipma(0, 0, &[1]);
        let raw = RawBox::new(&ipma).unwrap();
        IpmaVersionRecommendationRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn ip005_v1_unnecessary() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let ipma = make_ipma(1, 0, &[1, 2, 3]);
        let raw = RawBox::new(&ipma).unwrap();
        IpmaVersionRecommendationRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
        assert_eq!(ctx.diagnostics()[0].code(), "IP005");
    }

    #[test]
    fn ip005_v1_necessary() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let ipma = make_ipma(1, 0, &[70000]);
        let raw = RawBox::new(&ipma).unwrap();
        IpmaVersionRecommendationRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn ip006_flags_zero_no_warning() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let ipma = make_ipma(0, 0, &[1]);
        let raw = RawBox::new(&ipma).unwrap();
        IpmaFlagsRecommendationRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn ip006_flags_set_unnecessary() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().ipco_property_count = Some(10);
        let ipma = make_ipma(0, 1, &[1]);
        let raw = RawBox::new(&ipma).unwrap();
        IpmaFlagsRecommendationRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
        assert_eq!(ctx.diagnostics()[0].code(), "IP006");
    }

    #[test]
    fn ip006_flags_set_necessary() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().ipco_property_count = Some(200);
        let ipma = make_ipma(0, 1, &[1]);
        let raw = RawBox::new(&ipma).unwrap();
        IpmaFlagsRecommendationRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn ip006_exit_container_tracks_ipco_count() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = ContainerChildTracker::default();
        tracker.record_child(BoxCode::new(*b"ispe"));
        tracker.record_child(BoxCode::new(*b"pixi"));
        tracker.record_child(BoxCode::new(*b"colr"));
        ctx.state_mut().container_stack.push((BoxCode::IPCO, tracker));
        IpmaFlagsRecommendationRule.exit_container(&mut ctx, BoxCode::IPCO);
        assert_eq!(ctx.state().ipco_property_count, Some(3));
    }
}
