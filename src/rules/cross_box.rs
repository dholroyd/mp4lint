
use crate::context::ValidationContext;
use crate::diagnostic::{BoxPath, DiagnosticType, PropertyPath};
use crate::rules::ValidationRule;
use isobmff_syntax::FourCC;

pub struct ChunkOffsetsWithinMdatRule;

impl ValidationRule for ChunkOffsetsWithinMdatRule {
    fn code(&self) -> &'static str {
        "XB002"
    }

    fn name(&self) -> &'static str {
        "chunk-offsets-within-mdat"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        if state.mdat_ranges.is_empty() {
            return;
        }

        let mut errors: Vec<(u32, u64, usize, Option<BoxPath>)> = Vec::new();
        for (&track_id, info) in &state.track_info {
            let Some((min_offset, min_index, max_offset, max_index)) = info.chunk_offset_range else {
                continue;
            };
            for &(offset, index) in &[(min_offset, min_index), (max_offset, max_index)] {
                let within_mdat = state
                    .mdat_ranges
                    .iter()
                    .any(|&(start, end)| offset >= start && offset < end);
                if !within_mdat {
                    errors.push((track_id, offset, index, info.stco_path.clone()));
                    break;
                }
            }
        }

        for (track_id, offset, index, path) in errors {
            ctx.emit_at_property(
                DiagnosticType::ChunkOffsetOutsideMdat { track_id, offset },
                path,
                PropertyPath::from_indexed("offset", index),
            );
        }
    }
}

pub struct ChunkDataExtentRule;

impl ValidationRule for ChunkDataExtentRule {
    fn code(&self) -> &'static str {
        "XB008"
    }

    fn name(&self) -> &'static str {
        "chunk-data-within-mdat"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        if state.mdat_ranges.is_empty() {
            return;
        }

        let mut errors: Vec<(u32, u64, u64, Option<BoxPath>)> = Vec::new();
        for (&track_id, info) in &state.track_info {
            let Some((min_offset, _, _, _)) = info.chunk_offset_range else {
                continue;
            };
            let Some(total_size) = info.total_sample_data_size else {
                continue;
            };
            let chunk_end = min_offset.saturating_add(total_size);

            let mdat_end = state
                .mdat_ranges
                .iter()
                .find(|&&(start, end)| min_offset >= start && min_offset < end)
                .map(|&(_, end)| end);

            if let Some(mdat_end) = mdat_end {
                if chunk_end > mdat_end {
                    errors.push((track_id, chunk_end, mdat_end, info.stco_path.clone()));
                }
            }
        }

        for (track_id, chunk_end, mdat_end, path) in errors {
            ctx.emit_at(
                DiagnosticType::ChunkDataExceedsMdat {
                    track_id,
                    chunk_end,
                    mdat_end,
                },
                path,
            );
        }
    }
}

pub struct ChunkOffsetMonotonicityRule;

impl ValidationRule for ChunkOffsetMonotonicityRule {
    fn code(&self) -> &'static str {
        "XB009"
    }

    fn name(&self) -> &'static str {
        "chunk-offsets-monotonic"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let mut errors: Vec<(u32, usize, u64, u64, Option<BoxPath>)> = Vec::new();
        for (&track_id, info) in &state.track_info {
            if let Some((index, offset, prev_offset)) = info.first_non_monotonic_chunk {
                errors.push((track_id, index, offset, prev_offset, info.stco_path.clone()));
            }
        }

        for (track_id, index, offset, prev_offset, path) in errors {
            ctx.emit_at_property(
                DiagnosticType::ChunkOffsetsNotMonotonic {
                    track_id,
                    index,
                    offset,
                    prev_offset,
                },
                path,
                PropertyPath::from_indexed("offset", index),
            );
        }
    }
}

pub struct SaizSaioPairingRule;

impl ValidationRule for SaizSaioPairingRule {
    fn code(&self) -> &'static str {
        "XB003"
    }

    fn name(&self) -> &'static str {
        "saiz-saio-pairing"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let saiz_count = state.saiz_aux_types.len();
        let saio_count = state.saio_aux_types.len();

        if saiz_count != saio_count {
            ctx.emit(DiagnosticType::SaizSaioCountMismatch {
                saiz: saiz_count,
                saio: saio_count,
            });
        }
    }
}

pub struct TrackGroupIdConsistencyRule;

impl ValidationRule for TrackGroupIdConsistencyRule {
    fn code(&self) -> &'static str {
        "XB004"
    }

    fn name(&self) -> &'static str {
        "track-group-id-consistency"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let zero_groups: Vec<(u32, FourCC, Option<BoxPath>)> = state
            .track_groups
            .iter()
            .filter(|(_, _, group_id)| *group_id == 0)
            .map(|(track_id, group_type, _)| {
                let path = state.track_info.get(track_id).and_then(|i| i.trak_path.clone());
                (*track_id, *group_type, path)
            })
            .collect();

        for (track_id, group_type, path) in zero_groups {
            ctx.emit_at(DiagnosticType::TrackGroupIdZero { track_id, group_type }, path);
        }
    }
}

pub struct EntityGroupReferencesRule;

impl ValidationRule for EntityGroupReferencesRule {
    fn code(&self) -> &'static str {
        "XB005"
    }

    fn name(&self) -> &'static str {
        "entity-group-references"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let meta_path = state.meta_path.clone();
        let mut errors: Vec<(FourCC, u32, u32)> = Vec::new();

        for (group_type, group_id, entity_ids) in &state.entity_groups {
            for &entity_id in entity_ids {
                let is_track = state.seen_track_ids.contains(&entity_id);
                let is_item = state.item_ids.contains(&entity_id);
                if !is_track && !is_item {
                    errors.push((*group_type, *group_id, entity_id));
                }
            }
        }

        for (group_type, group_id, entity_id) in errors {
            ctx.emit_at(DiagnosticType::EntityGroupInvalidRef {
                group_type,
                group_id,
                entity_id,
            }, meta_path.clone());
        }
    }
}

pub struct IlocReferencesRule;

impl ValidationRule for IlocReferencesRule {
    fn code(&self) -> &'static str {
        "XB006"
    }

    fn name(&self) -> &'static str {
        "iloc-references"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let meta_path = state.meta_path.clone();
        let mut orphans: Vec<u32> = state
            .iloc_item_ids
            .iter()
            .filter(|id| !state.item_ids.contains(id))
            .copied()
            .collect();
        orphans.sort_unstable();

        for item_id in orphans {
            ctx.emit_at(DiagnosticType::IlocItemNotInIinf { item_id }, meta_path.clone());
        }
    }
}

pub struct PitmItemIdRule;

impl ValidationRule for PitmItemIdRule {
    fn code(&self) -> &'static str {
        "XB007"
    }

    fn name(&self) -> &'static str {
        "pitm-item-id"
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let state = ctx.state();
        let meta_path = state.meta_path.clone();
        if let Some(primary_id) = state.primary_item_id
            && !state.item_ids.contains(&primary_id)
        {
            ctx.emit_at(DiagnosticType::PrimaryItemNotInIinf { item_id: primary_id }, meta_path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::{TrackInfo, ValidationOptions, ValidationContext};

    #[test]
    fn xb002_offsets_within_mdat() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().mdat_ranges = vec![(100, 1000)];
        let mut info = TrackInfo::default();
        info.chunk_offset_range = Some((200, 0, 800, 1));
        ctx.state_mut().track_info.insert(1, info);
        ChunkOffsetsWithinMdatRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb002_offset_outside_mdat() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().mdat_ranges = vec![(100, 1000)];
        let mut info = TrackInfo::default();
        info.chunk_offset_range = Some((200, 0, 5000, 1)); // 5000 outside
        ctx.state_mut().track_info.insert(1, info);
        ChunkOffsetsWithinMdatRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
    }

    #[test]
    fn xb002_no_mdat() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut info = TrackInfo::default();
        info.chunk_offset_range = Some((200, 0, 200, 0));
        ctx.state_mut().track_info.insert(1, info);
        ChunkOffsetsWithinMdatRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb003_paired() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().saiz_aux_types = vec![None, None];
        ctx.state_mut().saio_aux_types = vec![None, None];
        SaizSaioPairingRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb003_unpaired() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().saiz_aux_types = vec![None, None];
        ctx.state_mut().saio_aux_types = vec![None];
        SaizSaioPairingRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
    }


    #[test]
    fn xb004_valid_track_groups() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let msrc = FourCC(*b"msrc");
        ctx.state_mut().track_groups.push((1, msrc, 1));
        ctx.state_mut().track_groups.push((2, msrc, 1));
        TrackGroupIdConsistencyRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb004_zero_group_id() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let msrc = FourCC(*b"msrc");
        ctx.state_mut().track_groups.push((1, msrc, 0));
        TrackGroupIdConsistencyRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
    }


    #[test]
    fn xb005_valid_entity_group() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().seen_track_ids.insert(1);
        ctx.state_mut().seen_track_ids.insert(2);
        let altr = FourCC(*b"altr");
        ctx.state_mut().entity_groups.push((altr, 1, vec![1, 2]));
        EntityGroupReferencesRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb005_entity_group_references_item() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().item_ids.insert(10);
        ctx.state_mut().item_ids.insert(20);
        let altr = FourCC(*b"altr");
        ctx.state_mut().entity_groups.push((altr, 1, vec![10, 20]));
        EntityGroupReferencesRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb005_entity_group_invalid_reference() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().seen_track_ids.insert(1);
        let altr = FourCC(*b"altr");
        ctx.state_mut().entity_groups.push((altr, 1, vec![1, 99]));
        EntityGroupReferencesRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
        assert_eq!(ctx.error_count(), 1);
    }


    #[test]
    fn xb006_iloc_items_in_iinf() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().item_ids.insert(1);
        ctx.state_mut().item_ids.insert(2);
        ctx.state_mut().iloc_item_ids.insert(1);
        ctx.state_mut().iloc_item_ids.insert(2);
        IlocReferencesRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb006_iloc_item_missing_from_iinf() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().item_ids.insert(1);
        ctx.state_mut().iloc_item_ids.insert(1);
        ctx.state_mut().iloc_item_ids.insert(99);
        IlocReferencesRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
        assert_eq!(ctx.error_count(), 1);
    }

    #[test]
    fn xb006_no_iloc() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        IlocReferencesRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }


    #[test]
    fn xb007_primary_item_exists() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().item_ids.insert(1);
        ctx.state_mut().primary_item_id = Some(1);
        PitmItemIdRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb007_primary_item_missing() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().item_ids.insert(1);
        ctx.state_mut().primary_item_id = Some(99);
        PitmItemIdRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
    }

    #[test]
    fn xb007_no_pitm() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        PitmItemIdRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }


    #[test]
    fn xb009_monotonic_offsets() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut info = TrackInfo::default();
        info.chunk_offset_range = Some((100, 0, 500, 2));
        info.first_non_monotonic_chunk = None; // all monotonic
        ctx.state_mut().track_info.insert(1, info);
        ChunkOffsetMonotonicityRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb009_duplicate_offset() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut info = TrackInfo::default();
        info.chunk_offset_range = Some((100, 0, 100, 1));
        info.first_non_monotonic_chunk = Some((1, 100, 100)); // duplicate
        ctx.state_mut().track_info.insert(1, info);
        ChunkOffsetMonotonicityRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "XB009");
    }

    #[test]
    fn xb009_decreasing_offset() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut info = TrackInfo::default();
        info.chunk_offset_range = Some((50, 1, 100, 0));
        info.first_non_monotonic_chunk = Some((1, 50, 100)); // decreasing
        ctx.state_mut().track_info.insert(1, info);
        ChunkOffsetMonotonicityRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "XB009");
    }


    #[test]
    fn xb008_data_within_mdat() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().mdat_ranges = vec![(100, 10000)];
        let mut info = TrackInfo::default();
        info.chunk_offset_range = Some((100, 0, 100, 0)); // single chunk at 100
        info.total_sample_data_size = Some(5000); // 100 + 5000 = 5100 < 10000
        ctx.state_mut().track_info.insert(1, info);
        ChunkDataExtentRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb008_data_exceeds_mdat() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().mdat_ranges = vec![(100, 10000)];
        let mut info = TrackInfo::default();
        info.chunk_offset_range = Some((100, 0, 100, 0)); // single chunk at 100
        info.total_sample_data_size = Some(20000); // 100 + 20000 = 20100 > 10000
        ctx.state_mut().track_info.insert(1, info);
        ChunkDataExtentRule.finalize(&mut ctx);
        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "XB008");
    }

    #[test]
    fn xb008_no_mdat_skips() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut info = TrackInfo::default();
        info.chunk_offset_range = Some((100, 0, 100, 0));
        info.total_sample_data_size = Some(20000);
        ctx.state_mut().track_info.insert(1, info);
        ChunkDataExtentRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn xb008_no_stsz_skips() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.state_mut().mdat_ranges = vec![(100, 10000)];
        let mut info = TrackInfo::default();
        info.chunk_offset_range = Some((100, 0, 100, 0));
        ctx.state_mut().track_info.insert(1, info);
        ChunkDataExtentRule.finalize(&mut ctx);
        assert!(!ctx.has_errors());
    }
}
