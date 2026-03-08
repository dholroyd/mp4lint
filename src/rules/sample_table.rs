
use crate::context::ValidationContext;
use crate::diagnostic::{BoxPath, DiagnosticType, PropertyPath};
use crate::rules::ValidationRule;
use isobmff_syntax::boxes::{
    CompactSampleSizeBox, CompactSampleSizeBoxView,
    CompositionTimeToSampleBox, CompositionTimeToSampleBoxView,
    SampleDescriptionBox, SampleDescriptionBoxView,
    SampleEntry, SampleEntryView,
    SampleToChunkBox, SampleToChunkBoxView,
    ShadowSyncSampleBox, ShadowSyncSampleBoxView,
    SyncSampleBox, SyncSampleBoxView,
    TimeToSampleBox, TimeToSampleBoxView,
};
use isobmff_syntax::{BoxCode, RawBox};

pub struct StblRequiredBoxesRule;

impl ValidationRule for StblRequiredBoxesRule {
    fn code(&self) -> &'static str {
        "E020"
    }

    fn name(&self) -> &'static str {
        "stbl-required-boxes"
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::STBL {
            return;
        }

        let missing: Vec<DiagnosticType> = {
            let Some(tracker) = ctx.current_container_tracker() else {
                return;
            };
            let mut msgs = Vec::new();
            if !tracker.has_child(BoxCode::STSD) {
                msgs.push(DiagnosticType::StblChildMissing { child: BoxCode::STSD });
            }
            if !tracker.has_child(BoxCode::STTS) {
                msgs.push(DiagnosticType::StblChildMissing { child: BoxCode::STTS });
            }
            if !tracker.has_child(BoxCode::STSC) {
                msgs.push(DiagnosticType::StblChildMissing { child: BoxCode::STSC });
            }
            if !tracker.has_child(BoxCode::STSZ) && !tracker.has_child(BoxCode::STZ2) {
                msgs.push(DiagnosticType::StblChildEitherMissing { child_a: BoxCode::STSZ, child_b: BoxCode::STZ2 });
            }
            if !tracker.has_child(BoxCode::STCO) && !tracker.has_child(BoxCode::CO64) {
                msgs.push(DiagnosticType::StblChildEitherMissing { child_a: BoxCode::STCO, child_b: BoxCode::CO64 });
            }
            msgs
        };

        for dt in missing {
            ctx.emit(dt);
        }
    }
}

pub struct ChunkOffsetExclusivityRule;

impl ValidationRule for ChunkOffsetExclusivityRule {
    fn code(&self) -> &'static str {
        "ST025"
    }

    fn name(&self) -> &'static str {
        "chunk-offset-exclusivity"
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::STBL {
            return;
        }

        let Some(tracker) = ctx.current_container_tracker() else {
            return;
        };

        if tracker.has_child(BoxCode::STCO) && tracker.has_child(BoxCode::CO64) {
            ctx.emit(DiagnosticType::StblBothChunkOffsetVariants);
        }
    }
}

pub struct SampleCountConsistencyRule;

impl ValidationRule for SampleCountConsistencyRule {
    fn code(&self) -> &'static str {
        "E021"
    }

    fn name(&self) -> &'static str {
        "sample-count-consistency"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() == BoxCode::STTS {
            let Ok(stts) = TimeToSampleBoxView::new(raw_box.data()) else {
                return;
            };
            let total = stts.entries().try_fold(0u64, |acc, e| {
                acc.checked_add(e.sample_count as u64)
            });
            let state = ctx.state_mut();
            if let Some(track_id) = state.current_track_id {
                let info = state.track_info.entry(track_id).or_default();
                info.stts_total_samples = total;
            }
            if total.is_none() {
                ctx.emit(DiagnosticType::SampleCountSumOverflow { box_type: BoxCode::STTS });
            }
        }
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        let mismatches: Vec<(u64, u64, Option<BoxPath>)> = {
            let state = ctx.state();
            state.track_info.iter().filter_map(|(&_track_id, info)| {
                if let (Some(stts_total), Some(stsz_count)) = (info.stts_total_samples, info.stsz_sample_count)
                    && stts_total != stsz_count as u64 {
                        return Some((stsz_count as u64, stts_total, info.trak_path.clone()));
                    }
                None
            }).collect()
        };

        for (stsz, stts, path) in mismatches {
            ctx.emit_at(DiagnosticType::SampleCountMismatch { stsz_count: stsz, stts_count: stts }, path);
        }
    }
}

pub struct StsdEntryCountRule;

impl ValidationRule for StsdEntryCountRule {
    fn code(&self) -> &'static str {
        "ST002"
    }

    fn name(&self) -> &'static str {
        "stsd-entry-count"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::STSD {
            return;
        }

        let Ok(stsd) = SampleDescriptionBoxView::new(raw_box.data()) else {
            return;
        };

        if stsd.entry_count() < 1 {
            ctx.emit_property(DiagnosticType::StsdEmpty, PropertyPath::from("entry_count"));
        }
    }
}

pub struct SttsPositiveDeltaRule;

impl ValidationRule for SttsPositiveDeltaRule {
    fn code(&self) -> &'static str {
        "ST005"
    }

    fn name(&self) -> &'static str {
        "stts-positive-delta"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::STTS {
            return;
        }

        let Ok(stts) = TimeToSampleBoxView::new(raw_box.data()) else {
            return;
        };

        let entries: Vec<_> = stts.entries().collect();
        for (i, entry) in entries.iter().enumerate() {
            if entry.sample_delta == 0 && i < entries.len() - 1 {
                ctx.emit_property(DiagnosticType::SttsZeroDelta { entry: i }, PropertyPath::from_indexed("entry", i).push("delta"));
            }
        }
    }
}

pub struct StscValidationRule;

impl ValidationRule for StscValidationRule {
    fn code(&self) -> &'static str {
        "ST012"
    }

    fn name(&self) -> &'static str {
        "stsc-validation"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::STSC {
            return;
        }

        let Ok(stsc) = SampleToChunkBoxView::new(raw_box.data()) else {
            return;
        };

        let entry_count = stsc.entry_count();
        if entry_count == 0 {
            return;
        }

        let (stsd_count, chunk_count, stsz_count) = {
            let info = ctx.state().current_track_id.and_then(|tid| {
                ctx.state().track_info.get(&tid)
            });
            match info {
                Some(info) => (
                    info.stsd_entry_count,
                    info.stco_entry_count,
                    info.stsz_sample_count,
                ),
                None => (None, None, None),
            }
        };

        let mut prev_first_chunk = 0u32;
        let mut total_samples: Option<u64> = Some(0);
        let mut last_entry_first_chunk = 0u32;
        let mut prev_samples_per_chunk: u64 = 0;

        for (i, entry) in stsc.entries().enumerate() {
            if entry.first_chunk <= prev_first_chunk && i > 0 {
                ctx.emit_property(DiagnosticType::StscFirstChunkNotIncreasing { entry: i, first_chunk: entry.first_chunk, prev: prev_first_chunk }, PropertyPath::from_indexed("entry", i).push("first_chunk"));
            }
            if entry.first_chunk == 0 {
                ctx.emit_property(DiagnosticType::StscFirstChunkZero { entry: i }, PropertyPath::from_indexed("entry", i).push("first_chunk"));
            }

            if let Some(stsd_count) = stsd_count
                && (entry.sample_description_index == 0 || entry.sample_description_index > stsd_count)
            {
                ctx.emit_property(DiagnosticType::StscSampleDescIndexOutOfRange { entry: i, value: entry.sample_description_index, max: stsd_count }, PropertyPath::from_indexed("entry", i).push("sample_desc_idx"));
            }

            if i > 0 && entry.first_chunk > prev_first_chunk
                && let Some(ref mut total) = total_samples
            {
                let chunks_in_run = (entry.first_chunk - prev_first_chunk) as u64;
                match chunks_in_run.checked_mul(prev_samples_per_chunk).and_then(|p| total.checked_add(p)) {
                    Some(new_total) => *total = new_total,
                    None => {
                        total_samples = None;
                        ctx.emit(DiagnosticType::StscSampleCountOverflow);
                    }
                }
            }

            prev_first_chunk = entry.first_chunk;
            prev_samples_per_chunk = entry.samples_per_chunk as u64;
            last_entry_first_chunk = entry.first_chunk;
        }

        if let Some(chunk_count) = chunk_count {
            if last_entry_first_chunk > chunk_count {
                ctx.emit(DiagnosticType::StscLastChunkExceedsCount { first_chunk: last_entry_first_chunk, chunk_count });
            }

            if let Some(stsz_count) = stsz_count {
                if chunk_count >= last_entry_first_chunk
                    && let Some(ref mut total) = total_samples
                {
                    let chunks_in_last_run = (chunk_count - last_entry_first_chunk + 1) as u64;
                    match chunks_in_last_run.checked_mul(prev_samples_per_chunk).and_then(|p| total.checked_add(p)) {
                        Some(new_total) => *total = new_total,
                        None => {
                            total_samples = None;
                            ctx.emit(DiagnosticType::StscSampleCountOverflow);
                        }
                    }
                }

                if let Some(total) = total_samples
                    && total != stsz_count as u64
                {
                    ctx.emit(DiagnosticType::StscSampleCountMismatch { stsc_total: total, stsz_count });
                }
            }
        }
    }
}

pub struct CttsCountConsistencyRule;

impl ValidationRule for CttsCountConsistencyRule {
    fn code(&self) -> &'static str {
        "ST015"
    }

    fn name(&self) -> &'static str {
        "ctts-count-consistency"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::CTTS {
            return;
        }

        let Ok(ctts) = CompositionTimeToSampleBoxView::new(raw_box.data()) else {
            return;
        };

        let ctts_total = ctts.entries().try_fold(0u64, |acc, e| {
            acc.checked_add(e.sample_count as u64)
        });

        let Some(ctts_total) = ctts_total else {
            ctx.emit(DiagnosticType::SampleCountSumOverflow { box_type: BoxCode::CTTS });
            return;
        };

        let stsz_count = ctx.state().current_track_id.and_then(|tid| {
            ctx.state().track_info.get(&tid).and_then(|info| info.stsz_sample_count)
        });

        if let Some(stsz_count) = stsz_count
            && ctts_total != stsz_count as u64 {
                ctx.emit(DiagnosticType::CttsSampleCountMismatch { ctts_total, stsz_count });
            }
    }
}

pub struct CttsAllZeroOffsetsRule;

impl ValidationRule for CttsAllZeroOffsetsRule {
    fn code(&self) -> &'static str {
        "ST016"
    }

    fn name(&self) -> &'static str {
        "ctts-all-zero-offsets"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::CTTS {
            return;
        }
        let Ok(ctts) = CompositionTimeToSampleBoxView::new(raw_box.data()) else {
            return;
        };
        if ctts.entry_count() == 0 {
            return;
        }
        if ctts.entries().all(|e| e.sample_offset == 0) {
            ctx.emit(DiagnosticType::CttsAllOffsetsZero);
        }
    }
}

pub struct UniqueCompositionTimestampsRule;

impl ValidationRule for UniqueCompositionTimestampsRule {
    fn code(&self) -> &'static str {
        "ST017"
    }

    fn name(&self) -> &'static str {
        "unique-composition-timestamps"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        isobmff_syntax::dispatch_box!(raw_box, {
            TimeToSampleBoxView(stts) => {
                let entries: Vec<_> = stts.entries().map(|e| (e.sample_count, e.sample_delta)).collect();
                let state = ctx.state_mut();
                if let Some(track_id) = state.current_track_id {
                    let info = state.track_info.entry(track_id).or_default();
                    info.stts_entries = entries;
                }
            },
            CompositionTimeToSampleBoxView(ctts) => {
                let entries: Vec<_> = ctts.entries().map(|e| (e.sample_count, e.sample_offset)).collect();
                let state = ctx.state_mut();
                if let Some(track_id) = state.current_track_id {
                    let info = state.track_info.entry(track_id).or_default();
                    info.ctts_entries = entries;
                }
            },
            _ => {},
            Err(_) => {}
        });
    }

    fn finalize(&self, ctx: &mut ValidationContext) {
        type DupResults = (Vec<(u32, i64, Option<BoxPath>)>, Vec<(u32, u64, Option<BoxPath>)>, Vec<(u32, Option<BoxPath>)>);
        let (results, skipped, overflowed): DupResults = {
            let state = ctx.state();
            let mut results = Vec::new();
            let mut skipped = Vec::new();
            let mut overflowed = Vec::new();
            for (&track_id, info) in &state.track_info {
                if info.stts_entries.is_empty() {
                    continue;
                }
                let Some(total_samples) = info.stts_entries.iter().try_fold(0u64, |acc, e| {
                    acc.checked_add(e.0 as u64)
                }) else {
                    overflowed.push((track_id, info.trak_path.clone()));
                    continue;
                };
                if total_samples == 0 {
                    continue;
                }
                if total_samples > 100_000 {
                    skipped.push((track_id, total_samples, info.trak_path.clone()));
                    continue;
                }

                if info.ctts_entries.is_empty() {
                    let mut prev_delta_zero = false;
                    let mut dup_dts = None;
                    let mut dts: i64 = 0;
                    let mut dts_overflowed = false;
                    for &(count, delta) in &info.stts_entries {
                        if delta == 0 && (count > 1 || prev_delta_zero) {
                            dup_dts = Some(dts);
                            break;
                        }
                        match (count as i64).checked_mul(delta as i64).and_then(|p| dts.checked_add(p)) {
                            Some(new_dts) => dts = new_dts,
                            None => {
                                dts_overflowed = true;
                                break;
                            }
                        }
                        prev_delta_zero = delta == 0;
                    }
                    if dts_overflowed {
                        overflowed.push((track_id, info.trak_path.clone()));
                    } else if let Some(ts) = dup_dts {
                        results.push((track_id, ts, info.trak_path.clone()));
                    }
                } else {
                    let mut ct_values = Vec::with_capacity(total_samples as usize);
                    let mut dts: i64 = 0;
                    let mut stts_iter = info.stts_entries.iter();
                    let mut ctts_iter = info.ctts_entries.iter();
                    let mut stts_remaining: u32 = 0;
                    let mut stts_delta: i64 = 0;
                    let mut ctts_remaining: u32 = 0;
                    let mut ctts_offset: i64 = 0;

                    for _ in 0..total_samples {
                        if stts_remaining == 0
                            && let Some(&(count, delta)) = stts_iter.next()
                        {
                            stts_remaining = count;
                            stts_delta = delta as i64;
                        }
                        if ctts_remaining == 0
                            && let Some(&(count, offset)) = ctts_iter.next()
                        {
                            ctts_remaining = count;
                            ctts_offset = offset;
                        }

                        ct_values.push(dts + ctts_offset);
                        dts += stts_delta;
                        stts_remaining -= 1;
                        ctts_remaining -= 1;
                    }

                    ct_values.sort_unstable();
                    for i in 1..ct_values.len() {
                        if ct_values[i] == ct_values[i - 1] {
                            results.push((track_id, ct_values[i], info.trak_path.clone()));
                            break;
                        }
                    }
                }
            }
            (results, skipped, overflowed)
        };

        for (track_id, path) in overflowed {
            ctx.emit_at(DiagnosticType::TimestampAccumulationOverflow { track_id }, path);
        }
        for (track_id, sample_count, path) in skipped {
            ctx.emit_at(DiagnosticType::DuplicateTimestampCheckSkipped { track_id, sample_count }, path);
        }
        for (_track_id, timestamp, path) in results {
            ctx.emit_at(DiagnosticType::DuplicateCompositionTimestamp { timestamp }, path);
        }
    }
}

pub struct StssStrictlyIncreasingRule;

impl ValidationRule for StssStrictlyIncreasingRule {
    fn code(&self) -> &'static str {
        "ST018"
    }

    fn name(&self) -> &'static str {
        "stss-strictly-increasing"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::STSS {
            return;
        }

        let Ok(stss) = SyncSampleBoxView::new(raw_box.data()) else {
            return;
        };

        let mut prev: u32 = 0;
        for (i, sample_num) in stss.entries().enumerate() {
            if i > 0 && sample_num <= prev {
                ctx.emit_property(DiagnosticType::StssNotIncreasing { entry: i, value: sample_num, prev }, PropertyPath::from_indexed("sync", i));
                break;
            }
            prev = sample_num;
        }
    }
}

pub struct StssValidRangeRule;

impl ValidationRule for StssValidRangeRule {
    fn code(&self) -> &'static str {
        "ST019"
    }

    fn name(&self) -> &'static str {
        "stss-valid-range"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::STSS {
            return;
        }

        let sample_count = ctx.state().current_track_id.and_then(|tid| {
            ctx.state().track_info.get(&tid).and_then(|info| info.stsz_sample_count)
        });

        let Ok(stss) = SyncSampleBoxView::new(raw_box.data()) else {
            return;
        };

        if let Some(sample_count) = sample_count {
            for (i, sample_num) in stss.entries().enumerate() {
                if sample_num == 0 || sample_num > sample_count {
                    ctx.emit_property(DiagnosticType::StssOutOfRange { entry: i, value: sample_num, sample_count }, PropertyPath::from_indexed("sync", i));
                    break;
                }
            }
        }
    }
}

pub struct StshSortedRule;

impl ValidationRule for StshSortedRule {
    fn code(&self) -> &'static str {
        "ST020"
    }

    fn name(&self) -> &'static str {
        "stsh-sorted"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::STSH {
            return;
        }

        let Ok(stsh) = ShadowSyncSampleBoxView::new(raw_box.data()) else {
            return;
        };

        let mut prev: u32 = 0;
        for (i, entry) in stsh.entries().enumerate() {
            if i > 0 && entry.shadowed_sample_number < prev {
                ctx.emit_property(DiagnosticType::StshNotSorted { entry: i, value: entry.shadowed_sample_number, prev }, PropertyPath::from_indexed("entry", i).push("shadowed"));
                break;
            }
            prev = entry.shadowed_sample_number;
        }
    }
}

pub struct Stz2FieldSizeRule;

impl ValidationRule for Stz2FieldSizeRule {
    fn code(&self) -> &'static str {
        "ST021"
    }

    fn name(&self) -> &'static str {
        "stz2-field-size"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::STZ2 {
            return;
        }

        let Ok(stz2) = CompactSampleSizeBoxView::new(raw_box.data()) else {
            return;
        };

        let field_size = stz2.field_size();
        if field_size != 4 && field_size != 8 && field_size != 16 {
            ctx.emit_property(DiagnosticType::Stz2InvalidFieldSize { field_size }, PropertyPath::from("field_size"));
        }
    }
}

pub struct SampleEntryDataRefIndexRule;

impl ValidationRule for SampleEntryDataRefIndexRule {
    fn code(&self) -> &'static str {
        "ST022"
    }

    fn name(&self) -> &'static str {
        "sample-entry-data-ref-index"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::STSD {
            return;
        }

        let Ok(stsd) = SampleDescriptionBoxView::new(raw_box.data()) else {
            return;
        };

        let dref_count = ctx.state().current_track_id.and_then(|tid| {
            ctx.state().track_info.get(&tid).and_then(|info| info.dref_entry_count)
        });

        let Some(dref_count) = dref_count else {
            return;
        };

        for (i, entry_raw) in stsd.entries().enumerate() {
            let Ok(entry) = SampleEntryView::new(entry_raw.data()) else {
                continue;
            };
            let dri = entry.data_reference_index();
            if dri == 0 || dri as u32 > dref_count {
                ctx.emit_property(DiagnosticType::SampleEntryDataRefIndexOutOfRange { entry: i, value: dri, max: dref_count }, PropertyPath::from_indexed("entry", i).push("data_reference_index"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ValidationOptions;

    fn make_stts(entries: &[(u32, u32)]) -> Vec<u8> {
        let size = 8 + 4 + 4 + entries.len() * 8;
        let mut data = Vec::with_capacity(size);
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"stts");
        data.extend_from_slice(&[0, 0, 0, 0]); // version + flags
        data.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for (count, delta) in entries {
            data.extend_from_slice(&count.to_be_bytes());
            data.extend_from_slice(&delta.to_be_bytes());
        }
        data
    }

    #[test]
    fn stbl_complete() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = crate::context::ContainerChildTracker::default();
        tracker.record_child(BoxCode::STSD);
        tracker.record_child(BoxCode::STTS);
        tracker.record_child(BoxCode::STSC);
        tracker.record_child(BoxCode::STSZ);
        tracker.record_child(BoxCode::STCO);
        ctx.state_mut().container_stack.push((BoxCode::STBL, tracker));

        StblRequiredBoxesRule.exit_container(&mut ctx, BoxCode::STBL);

        assert!(!ctx.has_errors());
    }

    #[test]
    fn st025_both_stco_and_co64() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = crate::context::ContainerChildTracker::default();
        tracker.record_child(BoxCode::STSD);
        tracker.record_child(BoxCode::STTS);
        tracker.record_child(BoxCode::STSC);
        tracker.record_child(BoxCode::STSZ);
        tracker.record_child(BoxCode::STCO);
        tracker.record_child(BoxCode::CO64);
        ctx.state_mut().container_stack.push((BoxCode::STBL, tracker));

        ChunkOffsetExclusivityRule.exit_container(&mut ctx, BoxCode::STBL);

        assert!(ctx.has_errors());
        assert!(ctx.diagnostics().iter().any(|d| d.code() == "ST025"));
    }

    #[test]
    fn st025_only_stco_ok() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = crate::context::ContainerChildTracker::default();
        tracker.record_child(BoxCode::STCO);
        ctx.state_mut().container_stack.push((BoxCode::STBL, tracker));

        ChunkOffsetExclusivityRule.exit_container(&mut ctx, BoxCode::STBL);

        assert!(!ctx.has_errors());
    }

    #[test]
    fn st025_only_co64_ok() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = crate::context::ContainerChildTracker::default();
        tracker.record_child(BoxCode::CO64);
        ctx.state_mut().container_stack.push((BoxCode::STBL, tracker));

        ChunkOffsetExclusivityRule.exit_container(&mut ctx, BoxCode::STBL);

        assert!(!ctx.has_errors());
    }

    #[test]
    fn stbl_missing_stsd() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = crate::context::ContainerChildTracker::default();
        tracker.record_child(BoxCode::STTS);
        tracker.record_child(BoxCode::STSC);
        tracker.record_child(BoxCode::STSZ);
        tracker.record_child(BoxCode::STCO);
        ctx.state_mut().container_stack.push((BoxCode::STBL, tracker));

        StblRequiredBoxesRule.exit_container(&mut ctx, BoxCode::STBL);

        assert!(ctx.has_errors());
        assert!(ctx.diagnostics()[0].message().contains("stsd"));
    }

    #[test]
    fn sample_count_mismatch() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let track_id = 1;
        ctx.state_mut().current_track_id = Some(track_id);
        let info = ctx.state_mut().track_info.entry(track_id).or_default();
        info.stsz_sample_count = Some(100);
        info.stts_total_samples = Some(50);

        let stts_data = make_stts(&[(50, 1024)]);
        let raw = isobmff_syntax::RawBox::new(&stts_data).unwrap();
        SampleCountConsistencyRule.validate_box(&mut ctx, &raw);

        SampleCountConsistencyRule.finalize(&mut ctx);

        assert!(ctx.has_errors());
        assert!(ctx.diagnostics()[0].message().contains("mismatch"));
    }

    #[test]
    fn sample_count_consistent() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let track_id = 1;
        ctx.state_mut().current_track_id = Some(track_id);
        let info = ctx.state_mut().track_info.entry(track_id).or_default();
        info.stsz_sample_count = Some(100);
        info.stts_total_samples = Some(100);

        SampleCountConsistencyRule.finalize(&mut ctx);

        assert!(!ctx.has_errors());
    }

    fn make_stsd_with_count(entry_count: u32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&16u32.to_be_bytes());
        data.extend_from_slice(b"stsd");
        data.extend_from_slice(&[0, 0, 0, 0]);
        data.extend_from_slice(&entry_count.to_be_bytes());
        data
    }

    #[test]
    fn st002_stsd_valid() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let stsd = make_stsd_with_count(1);
        let raw = isobmff_syntax::RawBox::new(&stsd).unwrap();
        StsdEntryCountRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn st002_stsd_zero() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let stsd = make_stsd_with_count(0);
        let raw = isobmff_syntax::RawBox::new(&stsd).unwrap();
        StsdEntryCountRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }

    fn make_stts_box(entries: &[(u32, u32)]) -> Vec<u8> {
        make_stts(entries)
    }

    #[test]
    fn st005_positive_deltas() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let stts = make_stts_box(&[(100, 1024)]);
        let raw = isobmff_syntax::RawBox::new(&stts).unwrap();
        SttsPositiveDeltaRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn st005_zero_delta_non_last() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let stts = make_stts_box(&[(10, 0), (90, 1024)]);
        let raw = isobmff_syntax::RawBox::new(&stts).unwrap();
        SttsPositiveDeltaRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn st005_zero_delta_last_ok() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let stts = make_stts_box(&[(99, 1024), (1, 0)]);
        let raw = isobmff_syntax::RawBox::new(&stts).unwrap();
        SttsPositiveDeltaRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    fn make_stss_box(entries: &[u32]) -> Vec<u8> {
        let size = 8 + 4 + 4 + entries.len() * 4;
        let mut data = Vec::with_capacity(size);
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"stss");
        data.extend_from_slice(&[0, 0, 0, 0]);
        data.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for &e in entries {
            data.extend_from_slice(&e.to_be_bytes());
        }
        data
    }

    #[test]
    fn st018_stss_increasing() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let stss = make_stss_box(&[1, 5, 10, 15]);
        let raw = isobmff_syntax::RawBox::new(&stss).unwrap();
        StssStrictlyIncreasingRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn st018_stss_not_increasing() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let stss = make_stss_box(&[1, 5, 3, 15]);
        let raw = isobmff_syntax::RawBox::new(&stss).unwrap();
        StssStrictlyIncreasingRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }

    fn make_stsc(entries: &[(u32, u32, u32)]) -> Vec<u8> {
        let size = 8 + 4 + 4 + entries.len() * 12;
        let mut data = Vec::with_capacity(size);
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"stsc");
        data.extend_from_slice(&[0, 0, 0, 0]); // version + flags
        data.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for &(first_chunk, samples_per_chunk, sample_desc_index) in entries {
            data.extend_from_slice(&first_chunk.to_be_bytes());
            data.extend_from_slice(&samples_per_chunk.to_be_bytes());
            data.extend_from_slice(&sample_desc_index.to_be_bytes());
        }
        data
    }

    fn make_ctts(entries: &[(u32, i64)]) -> Vec<u8> {
        let size = 8 + 4 + 4 + entries.len() * 8;
        let mut data = Vec::with_capacity(size);
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"ctts");
        data.extend_from_slice(&[1, 0, 0, 0]); // version 1 + flags
        data.extend_from_slice(&(entries.len() as u32).to_be_bytes());
        for &(count, offset) in entries {
            data.extend_from_slice(&count.to_be_bytes());
            data.extend_from_slice(&(offset as i32).to_be_bytes());
        }
        data
    }

    #[test]
    fn st016_ctts_all_zero_offsets() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let ctts = make_ctts(&[(10, 0), (20, 0), (30, 0)]);
        let raw = isobmff_syntax::RawBox::new(&ctts).unwrap();
        CttsAllZeroOffsetsRule.validate_box(&mut ctx, &raw);
        assert!(ctx.diagnostics().iter().any(|d| d.code() == "ST016"));
    }

    #[test]
    fn st016_ctts_nonzero_offsets_ok() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let ctts = make_ctts(&[(10, 1024), (20, 2048)]);
        let raw = isobmff_syntax::RawBox::new(&ctts).unwrap();
        CttsAllZeroOffsetsRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.diagnostics().iter().any(|d| d.code() == "ST016"));
    }

    #[test]
    fn st023_stsc_sample_count_overflow() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let track_id = 1;
        ctx.state_mut().current_track_id = Some(track_id);
        let info = ctx.state_mut().track_info.entry(track_id).or_default();
        info.stsd_entry_count = Some(1);

        let stsc = make_stsc(&[
            (1, u32::MAX, 1),
            (u32::MAX, u32::MAX, 1),   // chunks_in_run=MAX-1, product=(MAX-1)*MAX ≈ u64::MAX
            (1, u32::MAX, 1),          // non-monotonic, skips accumulation, resets prev_fc=1
            (u32::MAX, 1, 1),          // chunks_in_run=MAX-1, product=(MAX-1)*MAX → total overflows
        ]);
        let raw = isobmff_syntax::RawBox::new(&stsc).unwrap();
        StscValidationRule.validate_box(&mut ctx, &raw);
        assert!(ctx.diagnostics().iter().any(|d| d.code() == "ST023"),
            "Expected ST023 diagnostic for stsc sample count overflow");
    }

    #[test]
    fn st024_ctts_sample_count_sum_overflow() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let track_id = 1;
        ctx.state_mut().current_track_id = Some(track_id);
        let info = ctx.state_mut().track_info.entry(track_id).or_default();
        info.stsz_sample_count = Some(200);

        let ctts = make_ctts(&[(100, 10), (100, 20)]);
        let raw = isobmff_syntax::RawBox::new(&ctts).unwrap();
        CttsCountConsistencyRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.diagnostics().iter().any(|d| d.code() == "ST024"));
    }
}
