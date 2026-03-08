
use crate::context::ValidationContext;
use crate::diagnostic::{DiagnosticType, PropertyPath};
use crate::rules::ValidationRule;
use isobmff_syntax::boxes::{
    ChunkLargeOffsetBox, ChunkLargeOffsetBoxView,
    ChunkOffsetBox, ChunkOffsetBoxView,
    DataReferenceBox, DataReferenceBoxView,
    EditListBox, EditListBoxView,
    EntityGroupEntryBox, EntityGroupEntryBoxView,
    FileTypeBoxView,
    HandlerReferenceBox, HandlerReferenceBoxView,
    ItemInfoBoxView, ItemInfoEntryBox, ItemInfoEntryBoxView,
    ItemLocation, ItemLocationBox, ItemLocationBoxView,
    MediaHeaderBox, MediaHeaderBoxView,
    MovieFragmentHeaderBox, MovieFragmentHeaderBoxView,
    TrackReferenceTypeBox,
    MovieHeaderBox, MovieHeaderBoxView,
    PrimaryItemBox, PrimaryItemBoxView,
    SampleDescriptionBox, SampleDescriptionBoxView,
    SampleSizeBox, SampleSizeBoxView,
    TrackExtendsBox, TrackExtendsBoxView,
    TrackGroupTypeBox, TrackGroupTypeBoxView,
    TrackHeaderBox, TrackHeaderBoxView,
    TrackReferenceTypeBoxView,
    SampleAuxiliaryInformationSizesBox, SampleAuxiliaryInformationSizesBoxView,
    SampleAuxiliaryInformationOffsetsBox, SampleAuxiliaryInformationOffsetsBoxView,
    CompactSampleSizeBox, CompactSampleSizeBoxView,
};
use isobmff_syntax::{BoxCode, BoxInfo, RawBox, TrackReferenceCode};

pub struct StateAccumulatorRule;

impl ValidationRule for StateAccumulatorRule {
    fn code(&self) -> &'static str {
        "internal"
    }

    fn name(&self) -> &'static str {
        "state-accumulator"
    }

    fn observe_box(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
        match info.box_type {
            BoxCode::MDAT => {
                if let isobmff_syntax::BoxSize::Known(known) = info.size {
                    let end = info.offset.checked_add(known.get());
                    if let Some(end) = end {
                        ctx.state_mut().mdat_ranges.push((info.offset, end));
                    } else {
                        ctx.emit(crate::diagnostic::DiagnosticType::BoxExtentOverflow {
                            offset: info.offset,
                            size: known.get(),
                        });
                    }
                } else {
                    let file_size = ctx.state().file_size;
                    ctx.state_mut().mdat_ranges.push((info.offset, file_size));
                }
                if ctx.state().first_mdat_offset.is_none() {
                    ctx.state_mut().first_mdat_offset = Some(info.offset);
                }
            }
            BoxCode::MOOF => {
                ctx.state_mut().has_moof = true;
            }
            BoxCode::MFRA => {
                ctx.state_mut().mfra_offset = Some(info.offset);
                ctx.state_mut().mfra_size = Some(info.size);
            }
            _ => {}
        }
    }

    fn enter_container(&self, ctx: &mut ValidationContext, box_type: BoxCode, _info: &BoxInfo) {
        match box_type {
            BoxCode::EDTS => self.accumulate_edts(ctx),
            BoxCode::MVEX => {
                let path = ctx.path().clone();
                let state = ctx.state_mut();
                state.has_mvex = true;
                state.mvex_path = Some(path);
            }
            BoxCode::META => {
                if ctx.state().meta_path.is_none() {
                    let path = ctx.path().clone();
                    ctx.state_mut().meta_path = Some(path);
                }
            }
            _ => {}
        }
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        match raw_box.box_type() {
            BoxCode::FTYP => self.accumulate_ftyp(ctx, raw_box),
            BoxCode::MVHD => self.accumulate_mvhd(ctx, raw_box),
            BoxCode::TKHD => self.accumulate_tkhd(ctx, raw_box),
            BoxCode::MDHD => self.accumulate_mdhd(ctx, raw_box),
            BoxCode::HDLR => self.accumulate_hdlr(ctx, raw_box),
            BoxCode::ELST => self.accumulate_elst(ctx, raw_box),
            BoxCode::STSD => self.accumulate_stsd(ctx, raw_box),
            BoxCode::DREF => self.accumulate_dref(ctx, raw_box),
            BoxCode::STSZ => self.accumulate_stsz(ctx, raw_box),
            BoxCode::STZ2 => self.accumulate_stz2(ctx, raw_box),
            BoxCode::STCO => self.accumulate_stco(ctx, raw_box),
            BoxCode::CO64 => self.accumulate_co64(ctx, raw_box),
            BoxCode::TREX => self.accumulate_trex(ctx, raw_box),
            BoxCode::MFHD => self.accumulate_mfhd(ctx, raw_box),
            BoxCode::SAIZ => self.accumulate_saiz(ctx, raw_box),
            BoxCode::SAIO => self.accumulate_saio(ctx, raw_box),
            BoxCode::PITM => self.accumulate_pitm(ctx, raw_box),
            BoxCode::IINF => self.accumulate_iinf(ctx, raw_box),
            BoxCode::ILOC => self.accumulate_iloc(ctx, raw_box),
            _ => {
                let bt = raw_box.box_type();
                if bt == BoxCode::MEHD {
                    ctx.state_mut().has_mehd = true;
                }
                if ctx.path().contains(BoxCode::TREF) && bt != BoxCode::TREF {
                    self.accumulate_tref_child(ctx, raw_box);
                }
                if ctx.path().contains(BoxCode::TRGR) && bt != BoxCode::TRGR {
                    self.accumulate_trgr_child(ctx, raw_box);
                }
                if ctx.path().contains(BoxCode::GRPL) && bt != BoxCode::GRPL {
                    self.accumulate_grpl_child(ctx, raw_box);
                }
            }
        }
    }
}

impl StateAccumulatorRule {
    fn accumulate_ftyp(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(ftyp) = FileTypeBoxView::new(raw_box.data()) else {
            return;
        };
        let state = ctx.state_mut();
        state.compatible_brands = ftyp.compatible_brands().collect();
    }

    fn accumulate_mvhd(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(mvhd) = MovieHeaderBoxView::new(raw_box.data()) else {
            return;
        };
        let path = ctx.path().clone();
        let state = ctx.state_mut();
        state.mvhd_path = Some(path);
        state.mvhd_timescale = Some(mvhd.timescale());
        state.mvhd_duration = Some(mvhd.duration());
        state.next_track_id = Some(mvhd.next_track_id());
    }

    fn accumulate_tkhd(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(tkhd) = TrackHeaderBoxView::new(raw_box.data()) else {
            return;
        };
        let track_id = tkhd.track_id();
        let tkhd_path = ctx.path().clone();
        let trak_path = ctx.path().parent();
        let state = ctx.state_mut();
        state.current_track_id = Some(track_id);
        let info = state.track_info.entry(track_id).or_default();
        info.trak_path = Some(trak_path);
        info.tkhd_path = Some(tkhd_path);
        info.tkhd_duration = tkhd.duration();
        info.tkhd_flags = tkhd.flags();
        info.tkhd_width = tkhd.width().raw() as u32;
        info.tkhd_height = tkhd.height().raw() as u32;
        info.tkhd_volume = tkhd.volume().raw() as u16;
    }

    fn accumulate_mdhd(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(mdhd) = MediaHeaderBoxView::new(raw_box.data()) else {
            return;
        };
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            let info = state.track_info.entry(track_id).or_default();
            info.mdhd_duration = Some(mdhd.duration());
            info.mdhd_timescale = Some(mdhd.timescale());
        }
    }

    fn accumulate_hdlr(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if !ctx.path().contains(BoxCode::MDIA) {
            return;
        }
        let Ok(hdlr) = HandlerReferenceBoxView::new(raw_box.data()) else {
            return;
        };
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            let info = state.track_info.entry(track_id).or_default();
            info.handler_type = Some(hdlr.handler_type());
        }
    }

    fn accumulate_edts(&self, ctx: &mut ValidationContext) {
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            let info = state.track_info.entry(track_id).or_default();
            info.has_edts = true;
            state.tracks_with_edts.insert(track_id);
        }
    }

    fn accumulate_elst(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(elst) = EditListBoxView::new(raw_box.data()) else {
            return;
        };
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            let info = state.track_info.entry(track_id).or_default();
            for i in 0..elst.entry_count() as usize {
                if let Some(entry) = elst.entry(i) {
                    info.edit_list_entries.push((
                        entry.segment_duration,
                        entry.media_time,
                        entry.media_rate.raw(),
                    ));
                }
            }
        }
    }

    fn accumulate_stsd(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(stsd) = SampleDescriptionBoxView::new(raw_box.data()) else {
            return;
        };
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            let info = state.track_info.entry(track_id).or_default();
            info.stsd_entry_count = Some(stsd.entry_count());
        }
    }

    fn accumulate_dref(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(dref) = DataReferenceBoxView::new(raw_box.data()) else {
            return;
        };
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            let info = state.track_info.entry(track_id).or_default();
            info.dref_entry_count = Some(dref.entry_count());
        }
    }

    fn accumulate_stsz(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(stsz) = SampleSizeBoxView::new(raw_box.data()) else {
            return;
        };
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            let info = state.track_info.entry(track_id).or_default();
            info.stsz_sample_count = Some(stsz.sample_count());
            if stsz.sample_size() != 0 {
                info.total_sample_data_size =
                    Some(stsz.sample_count() as u64 * stsz.sample_size() as u64);
            } else {
                info.total_sample_data_size =
                    Some(stsz.entries().map(|s| s as u64).sum());
            }
        }
    }

    fn accumulate_stz2(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(stz2) = CompactSampleSizeBoxView::new(raw_box.data()) else {
            return;
        };
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            let info = state.track_info.entry(track_id).or_default();
            let count = stz2.sample_count();
            info.stsz_sample_count = Some(count);
            info.total_sample_data_size = Some(
                (0..count as usize)
                    .filter_map(|i| stz2.sample_size(i))
                    .map(|s| s as u64)
                    .sum(),
            );
        }
    }

    fn accumulate_stco(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(stco) = ChunkOffsetBoxView::new(raw_box.data()) else {
            return;
        };
        let path = ctx.path().clone();
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            let info = state.track_info.entry(track_id).or_default();
            info.stco_entry_count = Some(stco.entry_count());
            info.stco_path = Some(path);
            let mut prev: Option<u64> = None;
            info.chunk_offset_range = stco.entries().enumerate().fold(None, |acc, (i, o)| {
                let o = o as u64;
                if info.first_non_monotonic_chunk.is_none() {
                    if let Some(p) = prev {
                        if o <= p {
                            info.first_non_monotonic_chunk = Some((i, o, p));
                        }
                    }
                }
                prev = Some(o);
                Some(match acc {
                    None => (o, i, o, i),
                    Some((min, min_i, max, max_i)) => {
                        let (min, min_i) = if o < min { (o, i) } else { (min, min_i) };
                        let (max, max_i) = if o > max { (o, i) } else { (max, max_i) };
                        (min, min_i, max, max_i)
                    }
                })
            });
        }
    }

    fn accumulate_co64(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(co64) = ChunkLargeOffsetBoxView::new(raw_box.data()) else {
            return;
        };
        let path = ctx.path().clone();
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            let info = state.track_info.entry(track_id).or_default();
            info.stco_entry_count = Some(co64.entry_count());
            info.stco_path = Some(path);
            let mut prev: Option<u64> = None;
            info.chunk_offset_range = co64.entries().enumerate().fold(None, |acc, (i, o)| {
                if info.first_non_monotonic_chunk.is_none() {
                    if let Some(p) = prev {
                        if o <= p {
                            info.first_non_monotonic_chunk = Some((i, o, p));
                        }
                    }
                }
                prev = Some(o);
                Some(match acc {
                    None => (o, i, o, i),
                    Some((min, min_i, max, max_i)) => {
                        let (min, min_i) = if o < min { (o, i) } else { (min, min_i) };
                        let (max, max_i) = if o > max { (o, i) } else { (max, max_i) };
                        (min, min_i, max, max_i)
                    }
                })
            });
        }
    }

    fn accumulate_trex(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(trex) = TrackExtendsBoxView::new(raw_box.data()) else {
            return;
        };
        ctx.state_mut().trex_track_ids.insert(trex.track_id());
    }

    fn accumulate_mfhd(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(mfhd) = MovieFragmentHeaderBoxView::new(raw_box.data()) else {
            return;
        };
        ctx.state_mut().moof_sequence_numbers.push(mfhd.sequence_number());
    }

    fn accumulate_saiz(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(saiz) = SampleAuxiliaryInformationSizesBoxView::new(raw_box.data()) else {
            return;
        };
        ctx.state_mut().saiz_aux_types.push(saiz.aux_info_type());
    }

    fn accumulate_saio(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(saio) = SampleAuxiliaryInformationOffsetsBoxView::new(raw_box.data()) else {
            return;
        };
        ctx.state_mut().saio_aux_types.push(saio.aux_info_type());
    }

    fn accumulate_tref_child(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(tref_type) = TrackReferenceTypeBoxView::new(raw_box.data()) else {
            return;
        };
        let ref_type = TrackReferenceCode(raw_box.box_type().0);
        let state = ctx.state_mut();
        let track_id = state.current_track_id;

        for id in tref_type.track_ids() {
            state.referenced_track_ids.push((id, ref_type));
        }

        if ref_type == TrackReferenceCode::HINT
            && let Some(tid) = track_id
                && let Some(info) = state.track_info.get_mut(&tid) {
                    info.has_hint_tref = true;
                }
    }

    fn accumulate_pitm(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(pitm) = PrimaryItemBoxView::new(raw_box.data()) else {
            return;
        };
        ctx.state_mut().primary_item_id = Some(pitm.item_id());
    }

    fn accumulate_iinf(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(iinf) = ItemInfoBoxView::new(raw_box.data()) else {
            return;
        };
        let state = ctx.state_mut();
        for infe_raw in iinf.infe_boxes() {
            if let Ok(infe) = ItemInfoEntryBoxView::new(infe_raw.data()) {
                state.item_ids.insert(infe.item_id());
            }
        }
    }

    fn accumulate_iloc(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(iloc) = ItemLocationBoxView::new(raw_box.data()) else {
            return;
        };
        for (i, item) in iloc.items().enumerate() {
            let item = match item {
                Ok(it) => it,
                Err(err) => {
                    ctx.emit_property(
                        DiagnosticType::BoxParseFailed { detail: err.to_string() },
                        PropertyPath::from_indexed("item", i),
                    );
                    return;
                }
            };
            ctx.state_mut().iloc_item_ids.insert(item.item_id());
        }
    }

    fn accumulate_trgr_child(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(tgt) = TrackGroupTypeBoxView::new(raw_box.data()) else {
            return;
        };
        let state = ctx.state_mut();
        if let Some(track_id) = state.current_track_id {
            state.track_groups.push((track_id, tgt.track_group_type(), tgt.track_group_id()));
        }
    }

    fn accumulate_grpl_child(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        let Ok(entry) = EntityGroupEntryBoxView::new(raw_box.data()) else {
            return;
        };
        ctx.state_mut().entity_groups.push((
            entry.grouping_type(),
            entry.group_id(),
            entry.entity_ids().collect(),
        ));
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
    fn accumulate_ftyp_brands() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut payload = Vec::new();
        payload.extend_from_slice(b"isom"); // major brand
        payload.extend_from_slice(&0u32.to_be_bytes()); // minor version
        payload.extend_from_slice(b"isom"); // compatible brand 1
        payload.extend_from_slice(b"iso2"); // compatible brand 2
        let ftyp = make_box(b"ftyp", &payload);
        let raw = RawBox::new(&ftyp).unwrap();
        StateAccumulatorRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.state().compatible_brands.len(), 2);
    }

    #[test]
    fn accumulate_mvhd() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mvhd = crate::rules::timing::tests::make_mvhd(48000);
        let raw = RawBox::new(&mvhd).unwrap();
        StateAccumulatorRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.state().mvhd_timescale, Some(48000));
        assert_eq!(ctx.state().mvhd_duration, Some(0));
    }

    #[test]
    fn accumulate_mdat_ranges() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let info = BoxInfo {
            box_type: BoxCode::MDAT,
            offset: 100,
            size: isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(500).unwrap()),
            header_size: 8,
        };
        StateAccumulatorRule.observe_box(&mut ctx, &info);
        assert_eq!(ctx.state().mdat_ranges, vec![(100, 600)]);
    }

    #[test]
    fn f009_mdat_extent_overflow() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let info = BoxInfo {
            box_type: BoxCode::MDAT,
            offset: u64::MAX,
            size: isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(1).unwrap()),
            header_size: 8,
        };
        StateAccumulatorRule.observe_box(&mut ctx, &info);
        assert!(ctx.state().mdat_ranges.is_empty(),
            "Should not push to mdat_ranges when offset + size overflows");
        assert!(ctx.diagnostics().iter().any(|d| d.code() == "F009"),
            "Expected F009 diagnostic for box extent overflow");
    }
}
