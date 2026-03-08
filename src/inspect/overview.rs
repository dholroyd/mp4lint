use serde::Serialize;

use isobmff_syntax::boxes::{
    ChunkOffsetBox, ChunkOffsetBoxView, FileTypeBox, FileTypeBoxView, HandlerReferenceBox,
    HandlerReferenceBoxView, MediaHeaderBox, MediaHeaderBoxView, MovieHeaderBox,
    MovieHeaderBoxView, SampleSizeBox, SampleSizeBoxView, TrackHeaderBox, TrackHeaderBoxView,
};
use isobmff_syntax::{BoxCode, SliceBoxIterator};

use crate::validator::{is_container_box, payload_skip_for};

#[derive(Debug, Clone, Serialize)]
pub struct FileOverview {
    pub file_size: u64,
    pub major_brand: Option<String>,
    pub minor_version: Option<u32>,
    pub compatible_brands: Vec<String>,
    pub timescale: Option<u32>,
    pub duration: Option<u64>,
    pub tracks: Vec<TrackSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackSummary {
    pub track_id: u32,
    pub handler_type: Option<String>,
    pub duration: u64,
    pub timescale: Option<u32>,
    pub codec: Option<String>,
    pub sample_count: Option<u32>,
    pub chunk_count: Option<u32>,
}

impl FileOverview {
    pub fn new(file_size: u64) -> Self {
        FileOverview {
            file_size,
            major_brand: None,
            minor_version: None,
            compatible_brands: Vec::new(),
            timescale: None,
            duration: None,
            tracks: Vec::new(),
        }
    }

    pub fn process_box(&mut self, box_data: &[u8]) {
        let mut iter = SliceBoxIterator::new(box_data);
        let info = match iter.next_info() {
            Ok(Some(info)) => info,
            _ => return,
        };

        match info.box_type {
            BoxCode::FTYP => {
                if let Ok(view) = FileTypeBoxView::new(box_data) {
                    self.major_brand = Some(view.major_brand().to_string());
                    self.minor_version = Some(view.minor_version());
                    self.compatible_brands = (0..view.compatible_brands_count())
                        .filter_map(|i| view.compatible_brand(i))
                        .map(|b| b.to_string())
                        .collect();
                }
            }
            BoxCode::MOOV => {
                if let Ok(()) = iter.enter_container(&info, 0) {
                    parse_moov_children(&mut iter, self);
                    iter.exit_container();
                }
            }
            _ => {}
        }
    }
}

fn parse_moov_children(iter: &mut SliceBoxIterator<'_>, overview: &mut FileOverview) {
    loop {
        let info = match iter.next_info() {
            Ok(Some(info)) => info,
            _ => break,
        };

        match info.box_type {
            BoxCode::MVHD => {
                let box_data = iter.load_box(&info);
                if let Ok(view) = MovieHeaderBoxView::new(box_data) {
                    overview.timescale = Some(view.timescale());
                    overview.duration = Some(view.duration());
                }
            }
            BoxCode::TRAK => {
                let mut track = TrackSummary {
                    track_id: 0,
                    handler_type: None,
                    duration: 0,
                    timescale: None,
                    codec: None,
                    sample_count: None,
                    chunk_count: None,
                };
                if let Ok(()) = iter.enter_container(&info, 0) {
                    parse_trak_children(iter, &mut track);
                    iter.exit_container();
                }
                overview.tracks.push(track);
            }
            _ if is_container_box(info.box_type) => {
                let payload_skip = payload_skip_for(info.box_type);
                if let Ok(()) = iter.enter_container(&info, payload_skip) {
                    iter.exit_container();
                }
            }
            _ => {
                iter.skip_box(&info);
            }
        }
    }
}

fn parse_trak_children(iter: &mut SliceBoxIterator<'_>, track: &mut TrackSummary) {
    loop {
        let info = match iter.next_info() {
            Ok(Some(info)) => info,
            _ => break,
        };

        match info.box_type {
            BoxCode::TKHD => {
                let box_data = iter.load_box(&info);
                if let Ok(view) = TrackHeaderBoxView::new(box_data) {
                    track.track_id = view.track_id();
                    track.duration = view.duration();
                }
            }
            BoxCode::MDIA => {
                if let Ok(()) = iter.enter_container(&info, 0) {
                    parse_mdia_children(iter, track);
                    iter.exit_container();
                }
            }
            _ if is_container_box(info.box_type) => {
                let payload_skip = payload_skip_for(info.box_type);
                if let Ok(()) = iter.enter_container(&info, payload_skip) {
                    iter.exit_container();
                }
            }
            _ => {
                iter.skip_box(&info);
            }
        }
    }
}

fn parse_mdia_children(iter: &mut SliceBoxIterator<'_>, track: &mut TrackSummary) {
    loop {
        let info = match iter.next_info() {
            Ok(Some(info)) => info,
            _ => break,
        };

        match info.box_type {
            BoxCode::MDHD => {
                let box_data = iter.load_box(&info);
                if let Ok(view) = MediaHeaderBoxView::new(box_data) {
                    track.timescale = Some(view.timescale());
                }
            }
            BoxCode::HDLR => {
                let box_data = iter.load_box(&info);
                if let Ok(view) = HandlerReferenceBoxView::new(box_data) {
                    track.handler_type = Some(view.handler_type().to_string());
                }
            }
            BoxCode::MINF => {
                if let Ok(()) = iter.enter_container(&info, 0) {
                    parse_minf_children(iter, track);
                    iter.exit_container();
                }
            }
            _ if is_container_box(info.box_type) => {
                let payload_skip = payload_skip_for(info.box_type);
                if let Ok(()) = iter.enter_container(&info, payload_skip) {
                    iter.exit_container();
                }
            }
            _ => {
                iter.skip_box(&info);
            }
        }
    }
}

fn parse_minf_children(iter: &mut SliceBoxIterator<'_>, track: &mut TrackSummary) {
    loop {
        let info = match iter.next_info() {
            Ok(Some(info)) => info,
            _ => break,
        };

        match info.box_type {
            BoxCode::STBL => {
                if let Ok(()) = iter.enter_container(&info, 0) {
                    parse_stbl_children(iter, track);
                    iter.exit_container();
                }
            }
            _ if is_container_box(info.box_type) => {
                let payload_skip = payload_skip_for(info.box_type);
                if let Ok(()) = iter.enter_container(&info, payload_skip) {
                    iter.exit_container();
                }
            }
            _ => {
                iter.skip_box(&info);
            }
        }
    }
}

fn parse_stbl_children(iter: &mut SliceBoxIterator<'_>, track: &mut TrackSummary) {
    loop {
        let info = match iter.next_info() {
            Ok(Some(info)) => info,
            _ => break,
        };

        match info.box_type {
            BoxCode::STSZ => {
                let box_data = iter.load_box(&info);
                if let Ok(view) = SampleSizeBoxView::new(box_data) {
                    track.sample_count = Some(view.sample_count());
                }
            }
            BoxCode::STCO => {
                let box_data = iter.load_box(&info);
                if let Ok(view) = ChunkOffsetBoxView::new(box_data) {
                    track.chunk_count = Some(view.entry_count());
                }
            }
            BoxCode::STSD => {
                let box_data = iter.load_box(&info);
                if let Ok(view) =
                    isobmff_syntax::boxes::SampleDescriptionBoxView::new(box_data)
                {
                    for child in view.entries() {
                        let bytes = child.box_type().0 .0;
                        let codec_str =
                            if bytes.iter().all(|&b| b.is_ascii_graphic() || b == b' ') {
                                String::from_utf8_lossy(&bytes).into_owned()
                            } else {
                                format!(
                                    "0x{:02x}{:02x}{:02x}{:02x}",
                                    bytes[0], bytes[1], bytes[2], bytes[3]
                                )
                            };
                        track.codec = Some(codec_str);
                        break;
                    }
                }
            }
            _ => {
                iter.skip_box(&info);
            }
        }
    }
}
