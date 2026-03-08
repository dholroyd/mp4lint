
use crate::diagnostic::{BoxPath, Diagnostic, DiagnosticLocation, DiagnosticType, PropertyPath, Severity, SiblingCounts};
use isobmff_syntax::{BoxCode, BoxSize, BrandCode, FourCC, HandlerCode, ParseError, TrackReferenceCode};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct ValidationOptions {
    pub lenient: bool,
}

impl ValidationOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lenient(mut self, lenient: bool) -> Self {
        self.lenient = lenient;
        self
    }

}

#[derive(Debug, Default)]
pub struct ContainerChildTracker {
    pub children: Vec<BoxCode>,
    pub child_counts: HashMap<FourCC, usize>,
    pub box_info_size: BoxSize,
}

impl ContainerChildTracker {
    pub fn record_child(&mut self, box_type: BoxCode) {
        self.children.push(box_type);
        *self.child_counts.entry(box_type.0).or_insert(0) += 1;
    }

    pub fn has_child(&self, box_type: BoxCode) -> bool {
        self.child_counts.contains_key(&box_type.0)
    }

    pub fn child_count(&self, box_type: BoxCode) -> usize {
        self.child_counts.get(&box_type.0).copied().unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default)]
pub struct TrackInfo {
    pub trak_path: Option<BoxPath>,
    pub tkhd_path: Option<BoxPath>,
    pub tkhd_duration: u64,
    pub tkhd_flags: u32,
    pub mdhd_duration: Option<u64>,
    pub mdhd_timescale: Option<u32>,
    pub handler_type: Option<HandlerCode>,
    pub has_edts: bool,
    pub stsd_entry_count: Option<u32>,
    pub dref_entry_count: Option<u32>,
    pub stsz_sample_count: Option<u32>,
    pub stco_entry_count: Option<u32>,
    pub edit_list_entries: Vec<(u64, i64, i32)>,
    pub stco_path: Option<BoxPath>,
    pub chunk_offset_range: Option<(u64, usize, u64, usize)>,
    pub first_non_monotonic_chunk: Option<(usize, u64, u64)>,
    pub total_sample_data_size: Option<u64>,
    pub stts_total_samples: Option<u64>,
    pub stts_entries: Vec<(u32, u32)>,
    pub ctts_entries: Vec<(u32, i64)>,
    pub has_hint_tref: bool,
    pub tkhd_width: u32,
    pub tkhd_height: u32,
    pub tkhd_volume: u16,
}

#[derive(Debug, Default)]
pub struct BrandFeatures {
    pub seen_sdtp: bool,
    pub seen_sbgp: bool,
    pub seen_sgpd: bool,
    pub seen_meta: bool,
    pub seen_pdin: bool,
    pub seen_subs: bool,
    pub seen_fiin: bool,
    pub seen_ctts_v1: bool,
    pub seen_cslg_v0: bool,
    pub seen_trgr: bool,
    pub seen_idat: bool,
    pub seen_iref: bool,
    pub seen_tfdt: bool,
    pub seen_trun_v1: bool,
    pub seen_sidx: bool,
    pub seen_ssix: bool,
    pub seen_styp: bool,
    pub seen_prft: bool,
    pub seen_sgpd_in_traf: bool,
    pub seen_trep: bool,
    pub seen_sthd: bool,
    pub seen_meta_in_moof: bool,
    pub seen_elng: bool,
    pub seen_cslg_v1: bool,
    pub seen_iprp: bool,
    pub seen_grpl: bool,
}

#[derive(Debug, Default)]
pub struct ValidationState {
    pub has_ftyp: bool,

    pub ftyp_offset: Option<u64>,
    pub first_mdat_offset: Option<u64>,

    pub moov_count: usize,

    pub seen_track_ids: HashSet<u32>,

    pub has_media_track: bool,

    pub file_size: u64,
    pub compatible_brands: Vec<BrandCode>,
    pub mdat_ranges: Vec<(u64, u64)>,

    pub mvhd_path: Option<BoxPath>,
    pub mvhd_timescale: Option<u32>,
    pub mvhd_duration: Option<u64>,
    pub next_track_id: Option<u32>,

    pub current_track_id: Option<u32>,
    pub track_info: HashMap<u32, TrackInfo>,

    pub has_moof: bool,
    pub has_mvex: bool,
    pub mvex_path: Option<BoxPath>,
    pub has_mehd: bool,
    pub trex_track_ids: HashSet<u32>,
    pub moof_sequence_numbers: Vec<u32>,
    pub mfra_offset: Option<u64>,
    pub mfra_size: Option<BoxSize>,
    pub tracks_with_edts: HashSet<u32>,

    pub referenced_track_ids: Vec<(u32, TrackReferenceCode)>,

    pub saiz_aux_types: Vec<Option<FourCC>>,
    pub saio_aux_types: Vec<Option<FourCC>>,

    pub container_stack: Vec<(BoxCode, ContainerChildTracker)>,

    pub current_moof_base_offsets: Vec<Option<u64>>,
    pub current_moof_traf_modes: Vec<(bool, bool)>,

    pub track_groups: Vec<(u32, FourCC, u32)>,

    pub meta_path: Option<BoxPath>,
    pub item_ids: HashSet<u32>,
    pub primary_item_id: Option<u32>,
    pub entity_groups: Vec<(FourCC, u32, Vec<u32>)>,
    pub iloc_item_ids: HashSet<u32>,

    pub brand_features: BrandFeatures,

    pub first_moov_offset: Option<u64>,
    pub first_free_skip_offset: Option<u64>,

    pub ipma_item_ids: Vec<(u32, BoxPath)>,

    pub ipma_version_flags: Vec<(u8, u32, BoxPath)>,

    pub ipco_property_count: Option<u32>,

    pub top_level_tracker: ContainerChildTracker,
    pub sibling_counts: SiblingCounts,

    pub zero_sized_stack: Vec<Option<(FourCC, u64)>>,
}

pub struct ValidationContext {
    state: ValidationState,
    path: BoxPath,
    diagnostics: Vec<Diagnostic>,
    options: ValidationOptions,
}

impl ValidationContext {
    pub fn new(options: ValidationOptions) -> Self {
        let mut state = ValidationState::default();
        state.zero_sized_stack.push(None);
        Self {
            state,
            path: BoxPath::new(),
            diagnostics: Vec::new(),
            options,
        }
    }

    pub fn state_mut(&mut self) -> &mut ValidationState {
        &mut self.state
    }

    pub fn state(&self) -> &ValidationState {
        &self.state
    }

    pub fn path(&self) -> &BoxPath {
        &self.path
    }

    pub fn options(&self) -> &ValidationOptions {
        &self.options
    }

    pub fn push_path(&mut self, box_type: BoxCode) {
        self.path.push(box_type);
    }

    pub fn push_path_indexed(&mut self, box_type: BoxCode, index: usize) {
        self.path.push_indexed(box_type, index);
    }

    pub fn pop_path(&mut self) {
        self.path.pop();
    }

    pub fn emit(&mut self, dt: DiagnosticType) {
        if self.options.lenient && dt.severity() < Severity::Error {
            return;
        }
        let mut diagnostic = Diagnostic::new(dt);
        if !self.path.is_empty() {
            diagnostic.location = Some(DiagnosticLocation::Box(self.path.clone()));
        }
        self.diagnostics.push(diagnostic);
    }

    pub fn emit_property(&mut self, dt: DiagnosticType, property: PropertyPath) {
        if self.options.lenient && dt.severity() < Severity::Error {
            return;
        }
        let mut diagnostic = Diagnostic::new(dt);
        if !self.path.is_empty() {
            diagnostic.location = Some(DiagnosticLocation::Property(self.path.clone(), property));
        }
        self.diagnostics.push(diagnostic);
    }

    pub fn emit_at(&mut self, dt: DiagnosticType, location: Option<BoxPath>) {
        if self.options.lenient && dt.severity() < Severity::Error {
            return;
        }
        let mut diagnostic = Diagnostic::new(dt);
        if let Some(loc) = location {
            if !loc.is_empty() {
                diagnostic.location = Some(DiagnosticLocation::Box(loc));
            }
        }
        self.diagnostics.push(diagnostic);
    }

    pub fn emit_at_property(&mut self, dt: DiagnosticType, location: Option<BoxPath>, property: PropertyPath) {
        if self.options.lenient && dt.severity() < Severity::Error {
            return;
        }
        let mut diagnostic = Diagnostic::new(dt);
        if let Some(loc) = location {
            if !loc.is_empty() {
                diagnostic.location = Some(DiagnosticLocation::Property(loc, property));
            }
        }
        self.diagnostics.push(diagnostic);
    }

    pub fn emit_box_parse_error(&mut self, error: &ParseError) {
        self.emit(DiagnosticType::BoxParseFailed { detail: error.to_string() });
    }

    pub fn current_container(&self) -> Option<&(BoxCode, ContainerChildTracker)> {
        self.state.container_stack.last()
    }

    pub fn current_container_tracker(&self) -> Option<&ContainerChildTracker> {
        self.state.container_stack.last().map(|(_, t)| t)
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn into_diagnostics_and_sibling_counts(mut self) -> (Vec<Diagnostic>, SiblingCounts) {
        let top_counts = std::mem::take(&mut self.state.top_level_tracker.child_counts);
        self.state.sibling_counts.record("", top_counts);
        (self.diagnostics, self.state.sibling_counts)
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics.iter().any(|d| d.severity() == Severity::Error)
    }

    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity() == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity() == Severity::Warning)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_path_management() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        ctx.push_path(BoxCode::MOOV);
        assert_eq!(ctx.path().to_string(), "moov");

        ctx.push_path_indexed(BoxCode::TRAK, 0);
        assert_eq!(ctx.path().to_string(), "moov/trak[0]");

        ctx.pop_path();
        assert_eq!(ctx.path().to_string(), "moov");
    }

    #[test]
    fn lenient_mode_filters_warnings() {
        use crate::diagnostic::DiagnosticType;

        let options = ValidationOptions::new().lenient(true);
        let mut ctx = ValidationContext::new(options);

        ctx.emit(DiagnosticType::UnexpectedTopLevelBox {
            box_type: isobmff_syntax::FourCC(*b"free"),
            offset: 0,
        });
        ctx.emit(DiagnosticType::FtypMissing);

        assert_eq!(ctx.diagnostics().len(), 1);
        assert_eq!(ctx.diagnostics()[0].code(), "E001");
    }

    #[test]
    fn validation_state_defaults() {
        let state = ValidationState::default();

        assert!(!state.has_ftyp);
        assert!(state.ftyp_offset.is_none());
        assert!(state.first_mdat_offset.is_none());
        assert_eq!(state.moov_count, 0);
        assert!(state.seen_track_ids.is_empty());
        assert!(!state.has_media_track);
    }

    #[test]
    fn emit_at_with_path() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut path = BoxPath::new();
        path.push(BoxCode::MOOV);
        path.push(BoxCode::TRAK);
        ctx.emit_at(DiagnosticType::FtypMissing, Some(path.clone()));
        assert_eq!(ctx.diagnostics().len(), 1);
        let loc = ctx.diagnostics()[0].location.as_ref().unwrap();
        assert_eq!(loc.to_string(), "moov/trak");
    }

    #[test]
    fn emit_at_without_path() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.emit_at(DiagnosticType::FtypMissing, None);
        assert_eq!(ctx.diagnostics().len(), 1);
        assert!(ctx.diagnostics()[0].location.is_none());
    }

    #[test]
    fn emit_at_with_empty_path() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.emit_at(DiagnosticType::FtypMissing, Some(BoxPath::new()));
        assert_eq!(ctx.diagnostics().len(), 1);
        assert!(ctx.diagnostics()[0].location.is_none());
    }

    #[test]
    fn emit_at_lenient_filters_warnings() {
        let options = ValidationOptions::new().lenient(true);
        let mut ctx = ValidationContext::new(options);
        let mut path = BoxPath::new();
        path.push(BoxCode::MOOV);

        ctx.emit_at(
            DiagnosticType::MvhdNextTrackIdTooSmall { next_id: 1, max_id: 2 },
            Some(path.clone()),
        );
        assert_eq!(ctx.diagnostics().len(), 0);

        ctx.emit_at(DiagnosticType::FtypMissing, Some(path));
        assert_eq!(ctx.diagnostics().len(), 1);
    }

    #[test]
    fn validation_state_mutation() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());

        ctx.state_mut().has_ftyp = true;
        ctx.state_mut().moov_count = 1;
        ctx.state_mut().seen_track_ids.insert(1);
        ctx.state_mut().seen_track_ids.insert(2);

        assert!(ctx.state().has_ftyp);
        assert_eq!(ctx.state().moov_count, 1);
        assert_eq!(ctx.state().seen_track_ids.len(), 2);
    }
}
