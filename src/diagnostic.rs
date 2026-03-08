
use isobmff_syntax::{BoxCode, BrandCode, FourCC};
use serde::ser::SerializeStruct;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Info => write!(f, "info"),
            Severity::Warning => write!(f, "warning"),
            Severity::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathElement {
    Box(BoxCode),
    IndexedBox(BoxCode, usize),
}

impl fmt::Display for PathElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathElement::Box(code) => write!(f, "{}", code.0),
            PathElement::IndexedBox(code, index) => write!(f, "{}[{}]", code.0, index),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct BoxPath(Vec<PathElement>);

impl BoxPath {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push(&mut self, box_type: BoxCode) {
        self.0.push(PathElement::Box(box_type));
    }

    pub fn push_indexed(&mut self, box_type: BoxCode, index: usize) {
        self.0.push(PathElement::IndexedBox(box_type, index));
    }

    pub fn pop(&mut self) {
        self.0.pop();
    }

    pub fn parent(&self) -> BoxPath {
        let mut p = self.clone();
        p.pop();
        p
    }

    pub fn elements(&self) -> &[PathElement] {
        &self.0
    }

    pub fn contains(&self, box_type: BoxCode) -> bool {
        self.0.iter().any(|elem| match elem {
            PathElement::Box(code) | PathElement::IndexedBox(code, _) => *code == box_type,
        })
    }
}

impl fmt::Display for BoxPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, elem) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, "/")?;
            }
            write!(f, "{}", elem)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PropertyPathElement {
    Property(String),
    IndexedProperty(String, usize),
}

impl fmt::Display for PropertyPathElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PropertyPathElement::Property(name) => write!(f, "{}", name),
            PropertyPathElement::IndexedProperty(name, index) => write!(f, "{}[{}]", name, index),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PropertyPath(Vec<PropertyPathElement>);

impl PropertyPath {
    pub fn from(name: &str) -> Self {
        Self(vec![PropertyPathElement::Property(name.to_string())])
    }

    pub fn from_indexed(name: &str, index: usize) -> Self {
        Self(vec![PropertyPathElement::IndexedProperty(
            name.to_string(),
            index,
        )])
    }

    pub fn push(mut self, name: &str) -> Self {
        self.0.push(PropertyPathElement::Property(name.to_string()));
        self
    }

    pub fn push_indexed(mut self, name: &str, index: usize) -> Self {
        self.0.push(PropertyPathElement::IndexedProperty(
            name.to_string(),
            index,
        ));
        self
    }
}

impl fmt::Display for PropertyPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, elem) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ".")?;
            }
            write!(f, "{}", elem)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticLocation {
    Box(BoxPath),
    Property(BoxPath, PropertyPath),
}

impl DiagnosticLocation {
    pub fn box_path(&self) -> &BoxPath {
        match self {
            DiagnosticLocation::Box(path) => path,
            DiagnosticLocation::Property(path, _) => path,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.box_path().is_empty()
    }
}

impl fmt::Display for DiagnosticLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticLocation::Box(path) => write!(f, "{}", path),
            DiagnosticLocation::Property(path, prop) => write!(f, "{}.{}", path, prop),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SiblingCounts(HashMap<String, HashMap<FourCC, usize>>);

impl SiblingCounts {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn record(&mut self, container_path: &str, child_counts: HashMap<FourCC, usize>) {
        if child_counts.values().any(|&count| count > 1) {
            self.0.insert(container_path.to_string(), child_counts);
        }
    }

    pub fn format_path(&self, path: &BoxPath) -> String {
        use std::fmt::Write;
        let elements = path.elements();
        let mut result = String::new();
        let mut parent_key = String::new();

        for (i, elem) in elements.iter().enumerate() {
            if i > 0 {
                result.push('/');
            }

            let (code, index) = match elem {
                PathElement::Box(code) => (*code, None),
                PathElement::IndexedBox(code, idx) => (*code, Some(*idx)),
            };

            let needs_index = self
                .0
                .get(&parent_key)
                .and_then(|counts| counts.get(&code.0))
                .is_some_and(|&count| count > 1);

            if needs_index {
                if let Some(idx) = index {
                    write!(result, "{}[{}]", code.0, idx).unwrap();
                } else {
                    write!(result, "{}", code.0).unwrap();
                }
            } else {
                write!(result, "{}", code.0).unwrap();
            }

            if !parent_key.is_empty() {
                parent_key.push('/');
            }
            write!(parent_key, "{}", elem).unwrap();
        }

        result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFlagsLocation {
    TfhdDefault,
    TrexDefault,
}

impl fmt::Display for SampleFlagsLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SampleFlagsLocation::TfhdDefault => write!(f, "tfhd default_sample_flags"),
            SampleFlagsLocation::TrexDefault => write!(f, "trex default_sample_flags"),
        }
    }
}

fn box_display_name(code: BoxCode) -> &'static str {
    match code {
        BoxCode::MOOV => "MovieBox (moov)",
        BoxCode::MVHD => "MovieHeaderBox (mvhd)",
        BoxCode::TRAK => "TrackBox (trak)",
        BoxCode::TKHD => "TrackHeaderBox (tkhd)",
        BoxCode::MDIA => "MediaBox (mdia)",
        BoxCode::MDHD => "MediaHeaderBox (mdhd)",
        BoxCode::HDLR => "HandlerReferenceBox (hdlr)",
        BoxCode::MINF => "MediaInformationBox (minf)",
        BoxCode::STBL => "SampleTableBox (stbl)",
        BoxCode::STSD => "SampleDescriptionBox (stsd)",
        BoxCode::STTS => "TimeToSampleBox (stts)",
        BoxCode::STSC => "SampleToChunkBox (stsc)",
        BoxCode::STSZ => "SampleSizeBox (stsz)",
        BoxCode::STZ2 => "CompactSampleSizeBox (stz2)",
        BoxCode::STCO => "ChunkOffsetBox (stco)",
        BoxCode::CO64 => "ChunkLargeOffsetBox (co64)",
        BoxCode::DINF => "DataInformationBox (dinf)",
        BoxCode::DREF => "DataReferenceBox (dref)",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticType {
    FtypMissing,
    FtypAfterMdat,
    FtypAfterMoviebox,
    FtypAfterFreeSpace,
    MoovMissing,
    MoovDuplicate { count: usize },
    HierarchyChildMissing { parent: BoxCode, child: BoxCode },
    BoxSizeTooSmall { box_type: FourCC, offset: u64, size: u64, header_size: u32 },
    BoxTypeNonPrintable { offset: u64, bytes: [u8; 4] },
    ReservedBoxType { box_type: FourCC, offset: u64 },
    UnexpectedTopLevelBox { box_type: FourCC, offset: u64 },
    ZeroSizedBoxNotLast { box_type: FourCC, offset: u64 },

    TrackIdZero,
    TrackIdDuplicate { track_id: u32 },
    NoMediaTrack,
    TkhdInvalidVersion { version: u8 },
    TkhdUndefinedFlags { undefined_bits: u32 },
    TrefDuplicate { count: usize },
    TrefDuplicateType { ref_type: FourCC },

    StblChildMissing { child: BoxCode },
    StblChildEitherMissing { child_a: BoxCode, child_b: BoxCode },
    SampleCountMismatch { stsz_count: u64, stts_count: u64 },
    StscFirstChunkNotIncreasing { entry: usize, first_chunk: u32, prev: u32 },
    StscFirstChunkZero { entry: usize },
    StsdEmpty,
    SttsZeroDelta { entry: usize },
    StscSampleDescIndexOutOfRange { entry: usize, value: u32, max: u32 },
    StscLastChunkExceedsCount { first_chunk: u32, chunk_count: u32 },
    StscSampleCountMismatch { stsc_total: u64, stsz_count: u32 },
    CttsSampleCountMismatch { ctts_total: u64, stsz_count: u32 },
    CttsAllOffsetsZero,
    DuplicateCompositionTimestamp { timestamp: i64 },
    StssNotIncreasing { entry: usize, value: u32, prev: u32 },
    StssOutOfRange { entry: usize, value: u32, sample_count: u32 },
    StshNotSorted { entry: usize, value: u32, prev: u32 },
    Stz2InvalidFieldSize { field_size: u8 },
    SampleEntryDataRefIndexOutOfRange { entry: usize, value: u16, max: u32 },
    StblBothChunkOffsetVariants,

    MvhdParseFailed,
    MvhdTimescaleZero,
    MdhdParseFailed,
    MdhdTimescaleZero,
    MvhdNextTrackIdZero,
    MvhdNextTrackIdTooSmall { next_id: u32, max_id: u32 },
    MvhdInvalidVersion { version: u8 },
    MvhdDurationMismatch { mvhd_duration: u64, max_track: u64 },
    TrackDurationMismatch { track_id: u32, actual: u64, expected: u64 },
    EditListDurationMismatch { track_id: u32, sum: u64, tkhd_duration: u64 },
    EditListInvalidMediaTime { entry: usize, media_time: i64 },
    EditListUnusualMediaRate { entry: usize, rate_raw: i32 },
    IndefiniteDurationInconsistent,
    PtsStartWithoutEditList { track_id: u32, first_cts_offset: i64 },

    MinfMissingMediaHeader,
    MinfMissingDinf,
    DinfMissingDref,
    DrefEmpty,
    MdhdLanguageInvalid,
    ElngEmpty,
    ElngInvalidBcp47,
    UnknownHandlerType { handler: FourCC },
    HdlrNameNotNullTerminated,

    MoofMfhdMissing,
    MoofMfhdDuplicate { count: usize },
    SequenceNumberOutOfOrder { value: u32, position: usize, expected: u32 },
    MoofWithoutMvex,
    TrackMissingTrex { track_id: u32 },
    TrexTrackIdNotFound { track_id: u32 },
    TrafTfhdMissing,
    TrafTfhdDuplicate { count: usize },
    TfdtBeforeTfhd,
    TfdtAfterTrun,
    MvexWithoutMehd,
    DurationIsEmptyWithEdts { track_id: u32 },
    MoofMixedAddressing,
    MoofMixedMoofRelative,
    SampleFlagsReservedBits { location: SampleFlagsLocation },
    MfraNotAtEnd { mfra_end: u64, file_size: u64 },
    MfroSizeMismatch { declared: u64, actual: u64 },
    MfraMissingMfro,
    DefaultBaseIsMoofWithoutBrand,
    TfhdTrackIdInvalid { track_id: u32 },

    UnknownTrackRefType { ref_type: FourCC },
    DuplicateTrackIdInRef { track_id: u32, ref_type: FourCC },
    ZeroTrackIdInRef { ref_type: FourCC },
    ReferencedTrackNotFound { ref_id: u32, ref_type: FourCC },
    HintTrackMissingRef { track_id: u32 },
    ReservedTrackRefType { ref_type: FourCC },

    FullBoxInvalidVersion { box_type: FourCC, version: u8 },
    TkhdNonZeroReserved,
    MvhdNonZeroReserved,
    MatrixNonStandard { box_name: &'static str, value: i32 },
    VolumeNonStandard { value: u16 },
    NonVisualTrackDimensions { track_id: u32, handler: FourCC, width: u32, height: u32 },
    VmhdUndefinedFlags { flags: u32 },
    HdlrPreDefinedNonZero,

    ChunkOffsetOutsideMdat { track_id: u32, offset: u64 },
    ChunkDataExceedsMdat { track_id: u32, chunk_end: u64, mdat_end: u64 },
    ChunkOffsetsNotMonotonic { track_id: u32, index: usize, offset: u64, prev_offset: u64 },
    SaizSaioCountMismatch { saiz: usize, saio: usize },
    TrackGroupIdZero { track_id: u32, group_type: FourCC },
    EntityGroupInvalidRef { group_type: FourCC, group_id: u32, entity_id: u32 },
    IlocItemNotInIinf { item_id: u32 },
    PrimaryItemNotInIinf { item_id: u32 },

    BrandRequired { code_suffix: u8, box_name: &'static str, min_brand: BrandCode },

    PaspSpacingZero,

    IinfEntryOrderNotIncreasing { prev_id: u32, curr_id: u32 },
    IpmaItemIdNotIncreasing { prev_id: u32, curr_id: u32 },
    IpmaItemIdDuplicate { item_id: u32 },
    IpmaVersionFlagsDuplicate { version: u8, flags: u32 },
    IpmaVersionUnnecessary { version: u8 },
    IpmaFlagsUnnecessary,

    StscSampleCountOverflow,
    SampleCountSumOverflow { box_type: BoxCode },
    TimestampAccumulationOverflow { track_id: u32 },
    DurationComputationOverflow { track_id: u32, mdhd_duration: u64, mvhd_timescale: u32 },
    BoxExtentOverflow { offset: u64, size: u64 },

    BoxTooLargeToValidate { box_type: FourCC, size: u64, limit: u64 },
    DuplicateTimestampCheckSkipped { track_id: u32, sample_count: u64 },
    BoxParseFailed { detail: String },
}

const BRAND_CODES: [&str; 27] = [
    "",      // 0 unused
    "BR001", "BR002", "BR003", "BR004", "BR005", "BR006", "BR007",
    "BR008", "BR009", "BR010", "BR011", "BR012", "BR013", "BR014",
    "BR015", "BR016", "BR017", "BR018", "BR019", "BR020", "BR021",
    "BR022", "BR023", "BR024", "BR025", "BR026",
];

impl DiagnosticType {
    pub fn code(&self) -> &'static str {
        match self {
            DiagnosticType::FtypMissing => "E001",
            DiagnosticType::FtypAfterMdat
            | DiagnosticType::FtypAfterMoviebox
            | DiagnosticType::FtypAfterFreeSpace => "E002",
            DiagnosticType::MoovMissing | DiagnosticType::MoovDuplicate { .. } => "E003",
            DiagnosticType::HierarchyChildMissing { .. } => "E004",
            DiagnosticType::BoxSizeTooSmall { .. } => "F005",
            DiagnosticType::BoxTypeNonPrintable { .. } => "F006",
            DiagnosticType::ReservedBoxType { .. } => "F007",
            DiagnosticType::UnexpectedTopLevelBox { .. } => "F008",
            DiagnosticType::ZeroSizedBoxNotLast { .. } => "F010",

            DiagnosticType::NoMediaTrack => "T002",
            DiagnosticType::TrackIdDuplicate { .. } => "T004",
            DiagnosticType::TrackIdZero => "T005",
            DiagnosticType::TkhdInvalidVersion { .. } => "T007",
            DiagnosticType::TkhdUndefinedFlags { .. } => "T008",
            DiagnosticType::TrefDuplicate { .. } => "T009",
            DiagnosticType::TrefDuplicateType { .. } => "T010",

            DiagnosticType::StblChildMissing { .. }
            | DiagnosticType::StblChildEitherMissing { .. } => "E020",
            DiagnosticType::SampleCountMismatch { .. }
            | DiagnosticType::StscFirstChunkNotIncreasing { .. }
            | DiagnosticType::StscFirstChunkZero { .. } => "E021",
            DiagnosticType::StsdEmpty => "ST002",
            DiagnosticType::SttsZeroDelta { .. } => "ST005",
            DiagnosticType::StscSampleDescIndexOutOfRange { .. } => "ST012",
            DiagnosticType::StscLastChunkExceedsCount { .. } => "ST013",
            DiagnosticType::StscSampleCountMismatch { .. } => "ST014",
            DiagnosticType::CttsSampleCountMismatch { .. } => "ST015",
            DiagnosticType::CttsAllOffsetsZero => "ST016",
            DiagnosticType::DuplicateCompositionTimestamp { .. } => "ST017",
            DiagnosticType::StssNotIncreasing { .. } => "ST018",
            DiagnosticType::StssOutOfRange { .. } => "ST019",
            DiagnosticType::StshNotSorted { .. } => "ST020",
            DiagnosticType::Stz2InvalidFieldSize { .. } => "ST021",
            DiagnosticType::SampleEntryDataRefIndexOutOfRange { .. } => "ST022",
            DiagnosticType::StblBothChunkOffsetVariants => "ST025",

            DiagnosticType::MvhdParseFailed | DiagnosticType::MvhdTimescaleZero => "E030",
            DiagnosticType::MdhdParseFailed | DiagnosticType::MdhdTimescaleZero => "E030",
            DiagnosticType::MvhdNextTrackIdZero => "M003",
            DiagnosticType::MvhdNextTrackIdTooSmall { .. } => "M004",
            DiagnosticType::MvhdInvalidVersion { .. } => "M005",
            DiagnosticType::MvhdDurationMismatch { .. } => "M006",
            DiagnosticType::TrackDurationMismatch { .. } => "TM003",
            DiagnosticType::EditListDurationMismatch { .. } => "TM004",
            DiagnosticType::EditListInvalidMediaTime { .. } => "TM005",
            DiagnosticType::EditListUnusualMediaRate { .. } => "TM006",
            DiagnosticType::IndefiniteDurationInconsistent => "TM008",
            DiagnosticType::PtsStartWithoutEditList { .. } => "TM010",

            DiagnosticType::MinfMissingMediaHeader => "MD005",
            DiagnosticType::MinfMissingDinf => "MD007",
            DiagnosticType::DinfMissingDref => "MD008",
            DiagnosticType::DrefEmpty => "MD009",
            DiagnosticType::MdhdLanguageInvalid => "MD010",
            DiagnosticType::ElngEmpty | DiagnosticType::ElngInvalidBcp47 => "MD011",
            DiagnosticType::UnknownHandlerType { .. } => "MD012",
            DiagnosticType::HdlrNameNotNullTerminated => "MD013",

            DiagnosticType::MoofMfhdMissing | DiagnosticType::MoofMfhdDuplicate { .. } => "MF001",
            DiagnosticType::SequenceNumberOutOfOrder { .. } => "MF002",
            DiagnosticType::MoofWithoutMvex => "MF003",
            DiagnosticType::TrackMissingTrex { .. } => "MF004",
            DiagnosticType::TrexTrackIdNotFound { .. } => "MF005",
            DiagnosticType::TrafTfhdMissing | DiagnosticType::TrafTfhdDuplicate { .. } => "MF006",
            DiagnosticType::TfhdTrackIdInvalid { .. } => "MF007",
            DiagnosticType::TfdtBeforeTfhd | DiagnosticType::TfdtAfterTrun => "MF008",
            DiagnosticType::MvexWithoutMehd => "MF009",
            DiagnosticType::DurationIsEmptyWithEdts { .. } => "MF010",
            DiagnosticType::MoofMixedAddressing | DiagnosticType::MoofMixedMoofRelative => "MF011",
            DiagnosticType::SampleFlagsReservedBits { .. } => "MF012",
            DiagnosticType::MfraNotAtEnd { .. } => "MF013",
            DiagnosticType::MfroSizeMismatch { .. } | DiagnosticType::MfraMissingMfro => "MF014",
            DiagnosticType::DefaultBaseIsMoofWithoutBrand => "MF015",

            DiagnosticType::UnknownTrackRefType { .. } => "TR001",
            DiagnosticType::DuplicateTrackIdInRef { .. } => "TR002",
            DiagnosticType::ZeroTrackIdInRef { .. } => "TR003",
            DiagnosticType::ReferencedTrackNotFound { .. } => "TR004",
            DiagnosticType::HintTrackMissingRef { .. } => "TR005",
            DiagnosticType::ReservedTrackRefType { .. } => "F007b",

            DiagnosticType::FullBoxInvalidVersion { .. } => "FC001",
            DiagnosticType::TkhdNonZeroReserved | DiagnosticType::MvhdNonZeroReserved => "FC002",
            DiagnosticType::MatrixNonStandard { .. } => "FC003",
            DiagnosticType::VolumeNonStandard { .. } => "FC004",
            DiagnosticType::NonVisualTrackDimensions { .. } => "FC005",
            DiagnosticType::VmhdUndefinedFlags { .. } => "FC006",
            DiagnosticType::HdlrPreDefinedNonZero => "FC007",

            DiagnosticType::ChunkOffsetOutsideMdat { .. } => "XB002",
            DiagnosticType::ChunkDataExceedsMdat { .. } => "XB008",
            DiagnosticType::ChunkOffsetsNotMonotonic { .. } => "XB009",
            DiagnosticType::SaizSaioCountMismatch { .. } => "XB003",
            DiagnosticType::TrackGroupIdZero { .. } => "XB004",
            DiagnosticType::EntityGroupInvalidRef { .. } => "XB005",
            DiagnosticType::IlocItemNotInIinf { .. } => "XB006",
            DiagnosticType::PrimaryItemNotInIinf { .. } => "XB007",

            DiagnosticType::PaspSpacingZero => "VS001",

            DiagnosticType::IinfEntryOrderNotIncreasing { .. } => "IP001",
            DiagnosticType::IpmaItemIdNotIncreasing { .. } => "IP002",
            DiagnosticType::IpmaItemIdDuplicate { .. } => "IP003",
            DiagnosticType::IpmaVersionFlagsDuplicate { .. } => "IP004",
            DiagnosticType::IpmaVersionUnnecessary { .. } => "IP005",
            DiagnosticType::IpmaFlagsUnnecessary => "IP006",

            DiagnosticType::BrandRequired { code_suffix, .. } => BRAND_CODES[*code_suffix as usize],

            DiagnosticType::StscSampleCountOverflow => "ST023",
            DiagnosticType::SampleCountSumOverflow { .. } => "ST024",
            DiagnosticType::TimestampAccumulationOverflow { .. } => "ST025",
            DiagnosticType::DurationComputationOverflow { .. } => "TM009",
            DiagnosticType::BoxExtentOverflow { .. } => "F009",

            DiagnosticType::BoxTooLargeToValidate { .. } => "I001",
            DiagnosticType::DuplicateTimestampCheckSkipped { .. } => "I002",
            DiagnosticType::BoxParseFailed { .. } => "P001",
        }
    }

    pub fn severity(&self) -> Severity {
        match self {
            DiagnosticType::BoxTypeNonPrintable { .. }
            | DiagnosticType::ReservedBoxType { .. }
            | DiagnosticType::UnexpectedTopLevelBox { .. }
            | DiagnosticType::TkhdUndefinedFlags { .. }
            | DiagnosticType::SttsZeroDelta { .. }
            | DiagnosticType::DuplicateCompositionTimestamp { .. }
            | DiagnosticType::MvhdNextTrackIdTooSmall { .. }
            | DiagnosticType::MvhdDurationMismatch { .. }
            | DiagnosticType::TrackDurationMismatch { .. }
            | DiagnosticType::EditListDurationMismatch { .. }
            | DiagnosticType::EditListUnusualMediaRate { .. }
            | DiagnosticType::IndefiniteDurationInconsistent
            | DiagnosticType::PtsStartWithoutEditList { .. }
            | DiagnosticType::MdhdLanguageInvalid
            | DiagnosticType::ElngEmpty
            | DiagnosticType::ElngInvalidBcp47
            | DiagnosticType::UnknownHandlerType { .. }
            | DiagnosticType::SequenceNumberOutOfOrder { .. }
            | DiagnosticType::TfdtBeforeTfhd
            | DiagnosticType::TfdtAfterTrun
            | DiagnosticType::MvexWithoutMehd
            | DiagnosticType::SampleFlagsReservedBits { .. }
            | DiagnosticType::MfraNotAtEnd { .. }
            | DiagnosticType::UnknownTrackRefType { .. }
            | DiagnosticType::ReferencedTrackNotFound { .. }
            | DiagnosticType::ReservedTrackRefType { .. }
            | DiagnosticType::TkhdNonZeroReserved
            | DiagnosticType::MvhdNonZeroReserved
            | DiagnosticType::MatrixNonStandard { .. }
            | DiagnosticType::VolumeNonStandard { .. }
            | DiagnosticType::NonVisualTrackDimensions { .. }
            | DiagnosticType::VmhdUndefinedFlags { .. }
            | DiagnosticType::HdlrPreDefinedNonZero
            | DiagnosticType::HdlrNameNotNullTerminated
            | DiagnosticType::IinfEntryOrderNotIncreasing { .. }
            | DiagnosticType::IpmaVersionUnnecessary { .. }
            | DiagnosticType::IpmaFlagsUnnecessary
            | DiagnosticType::BrandRequired { .. } => Severity::Warning,

            DiagnosticType::BoxTooLargeToValidate { .. }
            | DiagnosticType::DuplicateTimestampCheckSkipped { .. } => Severity::Info,

            _ => Severity::Error,
        }
    }
}

impl fmt::Display for DiagnosticType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiagnosticType::FtypMissing => write!(f, "FileTypeBox (ftyp) is required but missing"),
            DiagnosticType::FtypAfterMdat => write!(f, "FileTypeBox (ftyp) should appear before MediaDataBox (mdat)"),
            DiagnosticType::FtypAfterMoviebox => write!(f, "FileTypeBox (ftyp) shall occur before MovieBox (moov)"),
            DiagnosticType::FtypAfterFreeSpace => write!(f, "FileTypeBox (ftyp) shall occur before FreeSpaceBox (free/skip)"),
            DiagnosticType::MoovMissing => write!(f, "MovieBox (moov) is required but missing"),
            DiagnosticType::MoovDuplicate { count } => write!(f, "Found {} MovieBox (moov) boxes, exactly one is required", count),
            DiagnosticType::HierarchyChildMissing { parent, child } => {
                if *parent == BoxCode::MOOV && *child == BoxCode::TRAK {
                    write!(f, "{} must contain at least one {}", box_display_name(*parent), box_display_name(*child))
                } else {
                    write!(f, "{} must contain {}", box_display_name(*parent), box_display_name(*child))
                }
            }
            DiagnosticType::BoxSizeTooSmall { box_type, offset, size, header_size } => write!(
                f, "Box '{}' at offset {} has size {} which is less than header size {}",
                box_type, offset, size, header_size
            ),
            DiagnosticType::BoxTypeNonPrintable { offset, bytes } => write!(
                f, "Box type at offset {} contains non-printable ASCII characters: {:?}",
                offset, bytes
            ),
            DiagnosticType::ReservedBoxType { box_type, offset } => write!(
                f, "Reserved box type '{}' found at offset {}", box_type, offset
            ),
            DiagnosticType::UnexpectedTopLevelBox { box_type, offset } => write!(
                f, "Unexpected top-level box type '{}' at offset {}", box_type, offset
            ),
            DiagnosticType::ZeroSizedBoxNotLast { box_type, offset } => write!(
                f, "Box '{}' at offset {} has size=0 but is not the last box in its container", box_type, offset
            ),

            DiagnosticType::TrackIdZero => write!(f, "Track has invalid track_id of 0"),
            DiagnosticType::TrackIdDuplicate { track_id } => write!(f, "Duplicate track_id {} found", track_id),
            DiagnosticType::NoMediaTrack => write!(f, "File must contain at least one media track (video or audio)"),
            DiagnosticType::TkhdInvalidVersion { version } => write!(
                f, "TrackHeaderBox (tkhd) has version {} (expected 0 or 1)", version
            ),
            DiagnosticType::TkhdUndefinedFlags { undefined_bits } => write!(
                f, "TrackHeaderBox (tkhd) has undefined flag bits set: {:#08x}", undefined_bits
            ),
            DiagnosticType::TrefDuplicate { count } => write!(
                f, "TrackBox (trak) contains {} TrackReferenceBox (tref) boxes, at most one allowed", count
            ),
            DiagnosticType::TrefDuplicateType { ref_type } => write!(
                f, "Duplicate reference type '{}' in TrackReferenceBox (tref)", ref_type
            ),

            DiagnosticType::StblChildMissing { child } => write!(
                f, "SampleTableBox (stbl) must contain {}", box_display_name(*child)
            ),
            DiagnosticType::StblChildEitherMissing { child_a, child_b } => write!(
                f, "SampleTableBox (stbl) must contain {} or {}", box_display_name(*child_a), box_display_name(*child_b)
            ),
            DiagnosticType::SampleCountMismatch { stsz_count, stts_count } => write!(
                f, "Sample count mismatch: stsz declares {} samples, stts declares {}", stsz_count, stts_count
            ),
            DiagnosticType::StscFirstChunkNotIncreasing { entry, first_chunk, prev } => write!(
                f, "stsc entry {} has first_chunk {} which is not greater than previous entry's first_chunk {}",
                entry, first_chunk, prev
            ),
            DiagnosticType::StscFirstChunkZero { entry } => write!(
                f, "stsc entry {} has invalid first_chunk of 0 (chunks are 1-indexed)", entry
            ),
            DiagnosticType::StsdEmpty => write!(
                f, "SampleDescriptionBox (stsd) must contain at least one entry"
            ),
            DiagnosticType::SttsZeroDelta { entry } => write!(
                f, "stts entry {} has zero sample_delta (non-last entry)", entry
            ),
            DiagnosticType::StscSampleDescIndexOutOfRange { entry, value, max } => write!(
                f, "stsc entry {} has sample_description_index {} which is out of range (valid: 1..{})",
                entry, value, max
            ),
            DiagnosticType::StscLastChunkExceedsCount { first_chunk, chunk_count } => write!(
                f, "Last stsc entry has first_chunk {} but stco/co64 has only {} chunks",
                first_chunk, chunk_count
            ),
            DiagnosticType::StscSampleCountMismatch { stsc_total, stsz_count } => write!(
                f, "Sample count from stsc+stco ({}) does not match stsz sample_count ({})",
                stsc_total, stsz_count
            ),
            DiagnosticType::CttsSampleCountMismatch { ctts_total, stsz_count } => write!(
                f, "ctts total sample count ({}) does not match stsz sample_count ({})",
                ctts_total, stsz_count
            ),
            DiagnosticType::CttsAllOffsetsZero => write!(
                f, "ctts has all-zero offsets; must not be present when composition time equals decoding time for every sample"
            ),
            DiagnosticType::DuplicateCompositionTimestamp { timestamp } => write!(
                f, "Duplicate composition timestamp {} found in track", timestamp
            ),
            DiagnosticType::StssNotIncreasing { entry, value, prev } => write!(
                f, "stss entry {} (value {}) is not strictly greater than previous ({})",
                entry, value, prev
            ),
            DiagnosticType::StssOutOfRange { entry, value, sample_count } => write!(
                f, "stss entry {} has value {} which is out of range (valid: 1..{})",
                entry, value, sample_count
            ),
            DiagnosticType::StshNotSorted { entry, value, prev } => write!(
                f, "stsh entry {} has shadowed_sample_number {} which is less than previous ({})",
                entry, value, prev
            ),
            DiagnosticType::Stz2InvalidFieldSize { field_size } => write!(
                f, "CompactSampleSizeBox (stz2) field_size {} is invalid (must be 4, 8, or 16)",
                field_size
            ),
            DiagnosticType::SampleEntryDataRefIndexOutOfRange { entry, value, max } => write!(
                f, "Sample entry {} has data_reference_index {} which is out of range (valid: 1..{})",
                entry, value, max
            ),
            DiagnosticType::StblBothChunkOffsetVariants => write!(
                f, "SampleTableBox (stbl) contains both stco and co64; at most one variant shall be present"
            ),

            DiagnosticType::MvhdParseFailed => write!(f, "Failed to parse MovieHeaderBox (mvhd)"),
            DiagnosticType::MvhdTimescaleZero => write!(f, "MovieHeaderBox (mvhd) timescale must be greater than 0"),
            DiagnosticType::MdhdParseFailed => write!(f, "Failed to parse MediaHeaderBox (mdhd)"),
            DiagnosticType::MdhdTimescaleZero => write!(f, "MediaHeaderBox (mdhd) timescale must be greater than 0"),
            DiagnosticType::MvhdNextTrackIdZero => write!(f, "MovieHeaderBox (mvhd) next_track_ID must be non-zero"),
            DiagnosticType::MvhdNextTrackIdTooSmall { next_id, max_id } => write!(
                f, "mvhd.next_track_ID ({}) should be greater than the largest track_ID ({})",
                next_id, max_id
            ),
            DiagnosticType::MvhdInvalidVersion { version } => write!(
                f, "MovieHeaderBox (mvhd) has version {} (expected 0 or 1)", version
            ),
            DiagnosticType::MvhdDurationMismatch { mvhd_duration, max_track } => write!(
                f, "mvhd.duration ({}) differs from longest track duration ({}) by more than 1 unit",
                mvhd_duration, max_track
            ),
            DiagnosticType::TrackDurationMismatch { track_id, actual, expected } => write!(
                f, "Track {} tkhd.duration ({}) differs from mdhd.duration in movie timescale ({}) by more than 1 unit",
                track_id, actual, expected
            ),
            DiagnosticType::EditListDurationMismatch { track_id, sum, tkhd_duration } => write!(
                f, "Track {} edit list duration sum ({}) does not match tkhd.duration ({})",
                track_id, sum, tkhd_duration
            ),
            DiagnosticType::EditListInvalidMediaTime { entry, media_time } => write!(
                f, "Edit list entry {} has invalid media_time {} (must be -1 or >= 0)",
                entry, media_time
            ),
            DiagnosticType::EditListUnusualMediaRate { entry, rate_raw } => write!(
                f, "Edit list entry {} has unusual media_rate {:#010x} (expected 0x00010000 for normal or 0 for dwell)",
                entry, rate_raw
            ),
            DiagnosticType::IndefiniteDurationInconsistent => write!(
                f, "Some tracks have indefinite duration but mvhd.duration is not indefinite"
            ),
            DiagnosticType::PtsStartWithoutEditList { track_id, first_cts_offset } => write!(
                f, "Track {} has non-zero initial composition offset ({}) but no edit list; presentation will not start at time zero",
                track_id, first_cts_offset
            ),

            DiagnosticType::MinfMissingMediaHeader => write!(
                f, "MediaInformationBox (minf) must contain a media-specific header (vmhd, smhd, hmhd, nmhd, or sthd)"
            ),
            DiagnosticType::MinfMissingDinf => write!(
                f, "MediaInformationBox (minf) must contain DataInformationBox (dinf)"
            ),
            DiagnosticType::DinfMissingDref => write!(
                f, "DataInformationBox (dinf) must contain DataReferenceBox (dref)"
            ),
            DiagnosticType::DrefEmpty => write!(
                f, "DataReferenceBox (dref) must contain at least one entry"
            ),
            DiagnosticType::MdhdLanguageInvalid => write!(
                f, "MediaHeaderBox (mdhd) language code is not valid ISO 639-2/T"
            ),
            DiagnosticType::ElngEmpty => write!(
                f, "ExtendedLanguageBox (elng) has empty language tag"
            ),
            DiagnosticType::ElngInvalidBcp47 => write!(
                f, "ExtendedLanguageBox (elng) language tag is not valid BCP 47"
            ),
            DiagnosticType::UnknownHandlerType { handler } => write!(
                f, "Unknown handler type '{}'", handler
            ),
            DiagnosticType::HdlrNameNotNullTerminated => write!(
                f, "HandlerReferenceBox (hdlr) name field is not null-terminated"
            ),

            DiagnosticType::MoofMfhdMissing => write!(
                f, "MovieFragmentBox (moof) must contain MovieFragmentHeaderBox (mfhd)"
            ),
            DiagnosticType::MoofMfhdDuplicate { count } => write!(
                f, "MovieFragmentBox (moof) contains {} mfhd boxes, exactly one required", count
            ),
            DiagnosticType::SequenceNumberOutOfOrder { value, position, expected } => write!(
                f, "Movie fragment sequence number {} at position {} (expected {})",
                value, position, expected
            ),
            DiagnosticType::MoofWithoutMvex => write!(
                f, "File contains movie fragments (moof) but MovieBox lacks MovieExtendsBox (mvex)"
            ),
            DiagnosticType::TrackMissingTrex { track_id } => write!(
                f, "Track {} has no corresponding TrackExtendsBox (trex) in mvex", track_id
            ),
            DiagnosticType::TrexTrackIdNotFound { track_id } => write!(
                f, "TrackExtendsBox (trex) references non-existent track_id {}", track_id
            ),
            DiagnosticType::TrafTfhdMissing => write!(
                f, "TrackFragmentBox (traf) must contain TrackFragmentHeaderBox (tfhd)"
            ),
            DiagnosticType::TrafTfhdDuplicate { count } => write!(
                f, "TrackFragmentBox (traf) contains {} tfhd boxes, exactly one required", count
            ),
            DiagnosticType::TfdtBeforeTfhd => write!(f, "tfdt should appear after tfhd in traf"),
            DiagnosticType::TfdtAfterTrun => write!(f, "tfdt should appear before first trun in traf"),
            DiagnosticType::MvexWithoutMehd => write!(
                f, "MovieExtendsBox (mvex) present without MovieExtendsHeaderBox (mehd); total duration must be computed from fragments"
            ),
            DiagnosticType::DurationIsEmptyWithEdts { track_id } => write!(
                f, "Track {} has duration-is-empty flag set in tfhd but also has an edit list", track_id
            ),
            DiagnosticType::MoofMixedAddressing => write!(
                f, "Inconsistent addressing within moof: some trafs use base-data-offset-present while others use implicit chaining"
            ),
            DiagnosticType::MoofMixedMoofRelative => write!(
                f, "Inconsistent addressing within moof: some trafs use default-base-is-moof while others use implicit chaining"
            ),
            DiagnosticType::SampleFlagsReservedBits { location } => write!(
                f, "{} has non-zero reserved bits (lower 4 bits)", location
            ),
            DiagnosticType::MfraNotAtEnd { mfra_end, file_size } => write!(
                f, "MovieFragmentRandomAccessBox (mfra) ends at {} but file size is {}; mfra should be the last box",
                mfra_end, file_size
            ),
            DiagnosticType::MfroSizeMismatch { declared, actual } => write!(
                f, "mfro.parent_size ({}) does not match mfra box size ({})",
                declared, actual
            ),
            DiagnosticType::MfraMissingMfro => write!(
                f, "MovieFragmentRandomAccessBox (mfra) must contain MovieFragmentRandomAccessOffsetBox (mfro)"
            ),
            DiagnosticType::DefaultBaseIsMoofWithoutBrand => write!(
                f, "default-base-is-moof flag (0x020000) in tfhd requires iso5 or later compatible brand"
            ),
            DiagnosticType::TfhdTrackIdInvalid { track_id } => write!(
                f, "TrackFragmentHeaderBox (tfhd) references non-existent track_id {}", track_id
            ),

            DiagnosticType::UnknownTrackRefType { ref_type } => write!(
                f, "Unknown track reference type '{}'", ref_type
            ),
            DiagnosticType::DuplicateTrackIdInRef { track_id, ref_type } => write!(
                f, "Duplicate track_id {} in track reference type '{}'", track_id, ref_type
            ),
            DiagnosticType::ZeroTrackIdInRef { ref_type } => write!(
                f, "Zero track_id in track reference type '{}'", ref_type
            ),
            DiagnosticType::ReferencedTrackNotFound { ref_id, ref_type } => write!(
                f, "Track reference type '{}' references non-existent track_id {}",
                ref_type, ref_id
            ),
            DiagnosticType::HintTrackMissingRef { track_id } => write!(
                f, "Hint track {} must have a 'hint' track reference to its source media track", track_id
            ),
            DiagnosticType::ReservedTrackRefType { ref_type } => write!(
                f, "Reserved track reference type '{}' used", ref_type
            ),

            DiagnosticType::FullBoxInvalidVersion { box_type, version } => write!(
                f, "{} has version {} (expected 0 or 1)", box_type, version
            ),
            DiagnosticType::TkhdNonZeroReserved => write!(
                f, "TrackHeaderBox (tkhd) has non-zero reserved field after track_id"
            ),
            DiagnosticType::MvhdNonZeroReserved => write!(
                f, "MovieHeaderBox (mvhd) has non-zero reserved fields"
            ),
            DiagnosticType::MatrixNonStandard { box_name, value } => write!(
                f, "{} matrix w[2] value {:#010x} is non-standard (expected 0x40000000)",
                box_name, value
            ),
            DiagnosticType::VolumeNonStandard { value } => write!(
                f, "TrackHeaderBox (tkhd) volume {:#06x} is non-standard (expected 0x0100 for audio or 0x0000 for visual)",
                value
            ),
            DiagnosticType::NonVisualTrackDimensions { track_id, handler, width, height } => write!(
                f, "Non-visual track {} (handler '{}') has non-zero dimensions ({}x{})",
                track_id, handler, width, height
            ),
            DiagnosticType::VmhdUndefinedFlags { flags } => write!(
                f, "VideoMediaHeaderBox (vmhd) has undefined flag bits set: {:#08x}", flags
            ),
            DiagnosticType::HdlrPreDefinedNonZero => write!(
                f, "HandlerReferenceBox (hdlr) pre_defined field should be zero"
            ),

            DiagnosticType::ChunkOffsetOutsideMdat { track_id, offset } => write!(
                f, "Track {} chunk offset {} does not fall within any mdat range", track_id, offset
            ),
            DiagnosticType::ChunkDataExceedsMdat { track_id, chunk_end, mdat_end } => write!(
                f, "Track {} sample data extends to offset {} which is past mdat end at {}", track_id, chunk_end, mdat_end
            ),
            DiagnosticType::ChunkOffsetsNotMonotonic { track_id, index, offset, prev_offset } => write!(
                f, "Track {} chunk offset[{}] ({}) is not greater than preceding offset ({}); chunk offsets must be strictly increasing",
                track_id, index, offset, prev_offset
            ),
            DiagnosticType::SaizSaioCountMismatch { saiz, saio } => write!(
                f, "Found {} saiz boxes but {} saio boxes; they should be paired", saiz, saio
            ),
            DiagnosticType::TrackGroupIdZero { track_id, group_type } => write!(
                f, "Track {} has track_group_id of 0 in group type '{}'; shall not be equal to zero",
                track_id, group_type
            ),
            DiagnosticType::EntityGroupInvalidRef { group_type, group_id, entity_id } => write!(
                f, "Entity group '{}' (group_id {}) references entity_id {} which is not a known track or item",
                group_type, group_id, entity_id
            ),
            DiagnosticType::IlocItemNotInIinf { item_id } => write!(
                f, "iloc references item_id {} which is not declared in iinf", item_id
            ),
            DiagnosticType::PrimaryItemNotInIinf { item_id } => write!(
                f, "Primary item ID {} from pitm does not exist in iinf", item_id
            ),

            DiagnosticType::BrandRequired { box_name, min_brand, .. } => write!(
                f, "{} requires {} or later compatible brand", box_name, min_brand.0
            ),

            DiagnosticType::PaspSpacingZero => write!(
                f, "PixelAspectRatioBox (pasp) hSpacing and vSpacing shall be strictly positive"
            ),

            DiagnosticType::IinfEntryOrderNotIncreasing { prev_id, curr_id } => write!(
                f, "ItemInfoBox (iinf) entries not sorted by increasing item_ID: {} followed by {}", prev_id, curr_id
            ),
            DiagnosticType::IpmaItemIdNotIncreasing { prev_id, curr_id } => write!(
                f, "ItemPropertyAssociationBox (ipma) entries not ordered by increasing item_ID: {} followed by {}", prev_id, curr_id
            ),
            DiagnosticType::IpmaItemIdDuplicate { item_id } => write!(
                f, "item_ID {} appears in multiple ItemPropertyAssociationBox (ipma) boxes", item_id
            ),
            DiagnosticType::IpmaVersionFlagsDuplicate { version, flags } => write!(
                f, "Duplicate ItemPropertyAssociationBox (ipma) with version={}, flags={:#x}", version, flags
            ),
            DiagnosticType::IpmaVersionUnnecessary { version } => write!(
                f, "ItemPropertyAssociationBox (ipma) version {} is unnecessary; version 0 should be used unless 32-bit item_IDs are needed", version
            ),
            DiagnosticType::IpmaFlagsUnnecessary => write!(
                f, "ItemPropertyAssociationBox (ipma) flags should be 0 unless there are more than 127 properties in ipco"
            ),

            DiagnosticType::StscSampleCountOverflow => write!(
                f, "stsc sample count computation overflowed: chunks_in_run \u{00d7} samples_per_chunk exceeds representable range"
            ),
            DiagnosticType::SampleCountSumOverflow { box_type } => write!(
                f, "Sum of sample_count values in {} overflows u64; values are unreasonably large", box_type.0
            ),
            DiagnosticType::TimestampAccumulationOverflow { track_id } => write!(
                f, "Track {} stts timestamp accumulation overflowed: sample_count \u{00d7} sample_delta exceeds i64 range",
                track_id
            ),
            DiagnosticType::DurationComputationOverflow { track_id, mdhd_duration, mvhd_timescale } => write!(
                f, "Track {} duration computation overflowed: mdhd.duration ({}) \u{00d7} mvhd.timescale ({}) exceeds u64 range",
                track_id, mdhd_duration, mvhd_timescale
            ),
            DiagnosticType::BoxExtentOverflow { offset, size } => write!(
                f, "Box at offset {} with size {} extends beyond u64 range", offset, size
            ),

            DiagnosticType::BoxTooLargeToValidate { box_type, size, limit } => write!(
                f, "Box '{}' is too large to validate ({} bytes, limit {} bytes)",
                box_type, size, limit
            ),
            DiagnosticType::DuplicateTimestampCheckSkipped { track_id, sample_count } => write!(
                f, "Duplicate composition timestamp check skipped for track {} ({} samples exceeds limit)",
                track_id, sample_count
            ),
            DiagnosticType::BoxParseFailed { detail } => write!(
                f, "Failed to parse box: {}", detail
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub diagnostic_type: DiagnosticType,
    pub location: Option<DiagnosticLocation>,
}

impl Diagnostic {
    pub fn new(diagnostic_type: DiagnosticType) -> Self {
        Self {
            diagnostic_type,
            location: None,
        }
    }

    pub fn severity(&self) -> Severity {
        self.diagnostic_type.severity()
    }

    pub fn code(&self) -> &'static str {
        self.diagnostic_type.code()
    }

    pub fn message(&self) -> String {
        self.diagnostic_type.to_string()
    }

    pub fn with_location(mut self, location: BoxPath) -> Self {
        self.location = Some(DiagnosticLocation::Box(location));
        self
    }

    pub fn with_property_location(mut self, location: BoxPath, property: PropertyPath) -> Self {
        self.location = Some(DiagnosticLocation::Property(location, property));
        self
    }

    pub fn with_optional_location(mut self, location: Option<BoxPath>) -> Self {
        self.location = location.map(DiagnosticLocation::Box);
        self
    }
}

impl Serialize for Diagnostic {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let has_location = self.location.is_some();
        let field_count = if has_location { 4 } else { 3 };
        let mut st = serializer.serialize_struct("Diagnostic", field_count)?;
        st.serialize_field("severity", &self.severity())?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.diagnostic_type.to_string())?;
        if let Some(ref loc) = self.location {
            st.serialize_field("location", &loc.to_string())?;
        }
        st.end()
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}: {}", self.code(), self.severity(), self.diagnostic_type)?;
        if let Some(ref loc) = self.location
            && !loc.is_empty() {
                write!(f, " at {}", loc)?;
            }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_path_operations() {
        let mut path = BoxPath::new();
        assert!(path.is_empty());

        path.push(BoxCode::MOOV);
        assert_eq!(path.to_string(), "moov");

        path.push_indexed(BoxCode::TRAK, 0);
        assert_eq!(path.to_string(), "moov/trak[0]");

        path.push(BoxCode::MDIA);
        assert_eq!(path.to_string(), "moov/trak[0]/mdia");

        path.pop();
        assert_eq!(path.to_string(), "moov/trak[0]");

        path.pop();
        assert_eq!(path.to_string(), "moov");

        path.pop();
        assert!(path.is_empty());
    }

    #[test]
    fn box_path_parent() {
        let mut path = BoxPath::new();
        path.push(BoxCode::MOOV);
        path.push_indexed(BoxCode::TRAK, 0);
        path.push(BoxCode::TKHD);

        let parent = path.parent();
        assert_eq!(parent.to_string(), "moov/trak[0]");

        let grandparent = parent.parent();
        assert_eq!(grandparent.to_string(), "moov");

        let root = grandparent.parent();
        assert!(root.is_empty());

        let empty_parent = root.parent();
        assert!(empty_parent.is_empty());
    }

    #[test]
    fn diagnostic_display() {
        let diag = Diagnostic::new(DiagnosticType::FtypMissing)
            .with_location(BoxPath::new());
        assert!(diag.to_string().contains("E001"));
        assert!(diag.to_string().contains("error"));
    }

    #[test]
    fn diagnostic_code_and_severity() {
        let diag = Diagnostic::new(DiagnosticType::FtypMissing);
        assert_eq!(diag.code(), "E001");
        assert_eq!(diag.severity(), Severity::Error);

        let diag = Diagnostic::new(DiagnosticType::BoxTypeNonPrintable { offset: 0, bytes: [0; 4] });
        assert_eq!(diag.code(), "F006");
        assert_eq!(diag.severity(), Severity::Warning);

        let diag = Diagnostic::new(DiagnosticType::BoxTooLargeToValidate {
            box_type: FourCC(*b"mdat"), size: 1000, limit: 100,
        });
        assert_eq!(diag.code(), "I001");
        assert_eq!(diag.severity(), Severity::Info);
    }

    #[test]
    fn diagnostic_message_lazy() {
        let diag = Diagnostic::new(DiagnosticType::MoovDuplicate { count: 3 });
        assert!(diag.message().contains("3"));
        assert!(diag.message().contains("moov"));
    }

    #[test]
    fn brand_required_codes() {
        for suffix in 1..=26u8 {
            let dt = DiagnosticType::BrandRequired {
                code_suffix: suffix,
                box_name: "test",
                min_brand: BrandCode::ISOM,
            };
            let expected = format!("BR{:03}", suffix);
            assert_eq!(dt.code(), expected.as_str());
        }
    }

    #[test]
    fn sibling_counts_format_path_multi_trak() {
        let mut counts = SiblingCounts::new();

        let mut moov_children = HashMap::new();
        moov_children.insert(FourCC(*b"trak"), 2);
        moov_children.insert(FourCC(*b"mvhd"), 1);
        counts.record("moov[0]", moov_children);

        let mut root_children = HashMap::new();
        root_children.insert(FourCC(*b"moov"), 1);
        root_children.insert(FourCC(*b"ftyp"), 1);
        counts.record("", root_children);

        let mut path = BoxPath::new();
        path.push_indexed(BoxCode::MOOV, 0);
        path.push_indexed(BoxCode::TRAK, 1);
        path.push_indexed(BoxCode::MDIA, 0);
        path.push_indexed(BoxCode::STBL, 0);

        assert_eq!(counts.format_path(&path), "moov/trak[1]/mdia/stbl");
    }

    #[test]
    fn sibling_counts_format_path_no_duplicates() {
        let counts = SiblingCounts::new();

        let mut path = BoxPath::new();
        path.push_indexed(BoxCode::MOOV, 0);
        path.push_indexed(BoxCode::TRAK, 0);
        path.push_indexed(BoxCode::MDIA, 0);

        assert_eq!(counts.format_path(&path), "moov/trak/mdia");
    }

    #[test]
    fn sibling_counts_format_path_empty() {
        let counts = SiblingCounts::new();
        let path = BoxPath::new();
        assert_eq!(counts.format_path(&path), "");
    }

    #[test]
    fn sibling_counts_record_skips_singletons() {
        let mut counts = SiblingCounts::new();

        let mut children = HashMap::new();
        children.insert(FourCC(*b"mvhd"), 1);
        children.insert(FourCC(*b"trak"), 1);
        counts.record("moov[0]", children);

        assert_eq!(counts.0.len(), 0);
    }

    #[test]
    fn box_path_elements() {
        let mut path = BoxPath::new();
        path.push(BoxCode::MOOV);
        path.push_indexed(BoxCode::TRAK, 1);

        let elems = path.elements();
        assert_eq!(elems.len(), 2);
        assert_eq!(elems[0], PathElement::Box(BoxCode::MOOV));
        assert_eq!(elems[1], PathElement::IndexedBox(BoxCode::TRAK, 1));
    }

    #[test]
    fn property_path_simple() {
        let pp = PropertyPath::from("language");
        assert_eq!(pp.to_string(), "language");
    }

    #[test]
    fn property_path_indexed() {
        let pp = PropertyPath::from_indexed("entry", 0);
        assert_eq!(pp.to_string(), "entry[0]");
    }

    #[test]
    fn property_path_nested() {
        let pp = PropertyPath::from_indexed("entry", 0).push("delta");
        assert_eq!(pp.to_string(), "entry[0].delta");
    }

    #[test]
    fn property_path_deeply_nested() {
        let pp = PropertyPath::from_indexed("entry", 0)
            .push_indexed("sub", 3)
            .push("value");
        assert_eq!(pp.to_string(), "entry[0].sub[3].value");
    }

    #[test]
    fn diagnostic_location_box() {
        let mut path = BoxPath::new();
        path.push(BoxCode::MOOV);
        path.push(BoxCode::MVHD);
        let loc = DiagnosticLocation::Box(path);
        assert_eq!(loc.to_string(), "moov/mvhd");
        assert!(!loc.is_empty());
    }

    #[test]
    fn diagnostic_location_property() {
        let mut path = BoxPath::new();
        path.push(BoxCode::MOOV);
        path.push(BoxCode::MDHD);
        let loc = DiagnosticLocation::Property(path, PropertyPath::from("language"));
        assert_eq!(loc.to_string(), "moov/mdhd.language");
        assert!(!loc.is_empty());
    }

    #[test]
    fn diagnostic_with_property_location() {
        let mut path = BoxPath::new();
        path.push(BoxCode::MOOV);
        path.push_indexed(BoxCode::TRAK, 0);
        path.push(BoxCode::MDIA);
        path.push(BoxCode::MDHD);
        let diag = Diagnostic::new(DiagnosticType::MdhdLanguageInvalid)
            .with_property_location(path, PropertyPath::from("language"));
        assert_eq!(
            diag.location.as_ref().unwrap().to_string(),
            "moov/trak[0]/mdia/mdhd.language"
        );
    }
}
