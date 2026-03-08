
use crate::context::{ContainerChildTracker, ValidationContext, ValidationOptions};
use crate::diagnostic::DiagnosticType;
use crate::report::ValidationReport;
use crate::rules::{DefaultRules, RuleSet};
use isobmff_syntax::{BoxCode, BoxReadError, RawBox, SliceBoxIterator, StreamingBoxIterator};
use std::io::{Read, Seek};

pub(crate) const MAX_BOX_SIZE: u64 = 100 * 1024 * 1024;

pub struct Validator<R: RuleSet = DefaultRules> {
    rules: R,
}

impl Validator<DefaultRules> {
    pub fn new() -> Self {
        Self {
            rules: DefaultRules,
        }
    }
}

impl<R: RuleSet> Validator<R> {
    pub fn with_rules(rules: R) -> Self {
        Self { rules }
    }

    pub fn rule_count(&self) -> usize {
        self.rules.rule_count()
    }

    pub fn validate<Rd: Read + Seek>(&self, reader: Rd) -> Result<ValidationReport, BoxReadError> {
        self.validate_with_options(reader, ValidationOptions::default())
    }

    pub fn validate_with_options<Rd: Read + Seek>(
        &self,
        reader: Rd,
        options: ValidationOptions,
    ) -> Result<ValidationReport, BoxReadError> {
        let mut ctx = ValidationContext::new(options);
        let mut iter = StreamingBoxIterator::new(reader)?;
        ctx.state_mut().file_size = iter.file_size();

        self.validate_stream(&mut iter, &mut ctx)?;

        self.rules.finalize(&mut ctx);

        let (diagnostics, sibling_counts) = ctx.into_diagnostics_and_sibling_counts();
        Ok(ValidationReport::from_diagnostics(diagnostics, sibling_counts))
    }

    pub fn validate_bytes(&self, data: &[u8]) -> ValidationReport {
        self.validate_bytes_with_options(data, ValidationOptions::default())
    }

    pub fn validate_bytes_with_options(
        &self,
        data: &[u8],
        options: ValidationOptions,
    ) -> ValidationReport {
        let mut ctx = ValidationContext::new(options);
        let mut iter = SliceBoxIterator::new(data);
        ctx.state_mut().file_size = iter.file_size();

        self.validate_slice(&mut iter, &mut ctx);

        self.rules.finalize(&mut ctx);

        let (diagnostics, sibling_counts) = ctx.into_diagnostics_and_sibling_counts();
        ValidationReport::from_diagnostics(diagnostics, sibling_counts)
    }

    fn validate_slice(
        &self,
        iter: &mut SliceBoxIterator<'_>,
        ctx: &mut ValidationContext,
    ) {
        loop {
            let info = match iter.next_info() {
                Ok(Some(info)) => info,
                Ok(None) => break,
                Err(e) => {
                    ctx.emit_box_parse_error(&e);
                    break;
                }
            };

            self.rules.observe_child(ctx, &info);

            if iter.depth() == 0 {
                self.rules.observe_box(ctx, &info);
            }

            let sibling_index = if let Some((_, tracker)) = ctx.state_mut().container_stack.last_mut() {
                tracker.record_child(info.box_type);
                tracker.child_count(info.box_type) - 1
            } else {
                let tl = &mut ctx.state_mut().top_level_tracker;
                tl.record_child(info.box_type);
                tl.child_count(info.box_type) - 1
            };

            if is_container_box(info.box_type) {
                let box_type = info.box_type;

                ctx.push_path_indexed(box_type, sibling_index);

                let tracker = ContainerChildTracker { box_info_size: info.size, ..Default::default() };
                ctx.state_mut().container_stack.push((box_type, tracker));

                self.rules.enter_container(ctx, box_type, &info);

                let payload_skip = payload_skip_for(box_type);
                if let Err(e) = iter.enter_container(&info, payload_skip) {
                    ctx.emit_box_parse_error(&e);
                    iter.skip_box(&info);
                } else {
                    self.validate_slice(iter, ctx);
                    iter.exit_container();
                }

                self.rules.exit_container(ctx, box_type);

                let container_path = ctx.path().to_string();
                let child_counts = ctx
                    .state()
                    .container_stack
                    .last()
                    .map(|(_, tracker)| tracker.child_counts.clone());
                if let Some(counts) = child_counts {
                    ctx.state_mut().sibling_counts.record(&container_path, counts);
                }

                ctx.state_mut().container_stack.pop();
                ctx.pop_path();
            } else if iter.effective_size(&info) <= MAX_BOX_SIZE {
                let data = iter.load_box(&info);
                match RawBox::new(data) {
                    Ok(raw_box) => {
                        ctx.push_path_indexed(raw_box.box_type(), sibling_index);
                        self.rules.validate_box(ctx, &raw_box);
                        ctx.pop_path();
                    }
                    Err(e) => {
                        ctx.push_path_indexed(info.box_type, sibling_index);
                        ctx.emit_box_parse_error(&e);
                        ctx.pop_path();
                    }
                }
            } else {
                ctx.push_path_indexed(info.box_type, sibling_index);
                ctx.emit(DiagnosticType::BoxTooLargeToValidate {
                    box_type: info.box_type.0,
                    size: iter.effective_size(&info),
                    limit: MAX_BOX_SIZE,
                });
                ctx.pop_path();
                iter.skip_box(&info);
            }
        }
    }

    fn validate_stream<Rd: Read + Seek>(
        &self,
        iter: &mut StreamingBoxIterator<Rd>,
        ctx: &mut ValidationContext,
    ) -> Result<(), BoxReadError> {
        loop {
            let info = match iter.next_info() {
                Ok(Some(info)) => info,
                Ok(None) => break,
                Err(BoxReadError::Parse(e)) => {
                    ctx.emit_box_parse_error(&e);
                    break;
                }
                Err(e @ BoxReadError::Io(_)) => return Err(e),
            };

            self.rules.observe_child(ctx, &info);

            if iter.depth() == 0 {
                self.rules.observe_box(ctx, &info);
            }

            let sibling_index = if let Some((_, tracker)) = ctx.state_mut().container_stack.last_mut() {
                tracker.record_child(info.box_type);
                tracker.child_count(info.box_type) - 1
            } else {
                let tl = &mut ctx.state_mut().top_level_tracker;
                tl.record_child(info.box_type);
                tl.child_count(info.box_type) - 1
            };

            if is_container_box(info.box_type) {
                let box_type = info.box_type;

                ctx.push_path_indexed(box_type, sibling_index);

                let tracker = ContainerChildTracker { box_info_size: info.size, ..Default::default() };
                ctx.state_mut().container_stack.push((box_type, tracker));

                self.rules.enter_container(ctx, box_type, &info);

                let payload_skip = payload_skip_for(box_type);
                match iter.enter_container(&info, payload_skip) {
                    Ok(()) => {
                        self.validate_stream(iter, ctx)?;
                        iter.exit_container()?;
                    }
                    Err(BoxReadError::Parse(e)) => {
                        ctx.emit_box_parse_error(&e);
                        iter.skip_box(&info)?;
                    }
                    Err(e @ BoxReadError::Io(_)) => {
                        self.rules.exit_container(ctx, box_type);
                        ctx.state_mut().container_stack.pop();
                        ctx.pop_path();
                        return Err(e);
                    }
                }

                self.rules.exit_container(ctx, box_type);

                let container_path = ctx.path().to_string();
                let child_counts = ctx
                    .state()
                    .container_stack
                    .last()
                    .map(|(_, tracker)| tracker.child_counts.clone());
                if let Some(counts) = child_counts {
                    ctx.state_mut().sibling_counts.record(&container_path, counts);
                }

                ctx.state_mut().container_stack.pop();
                ctx.pop_path();
            } else if iter.effective_size(&info) <= MAX_BOX_SIZE {
                let data = match iter.load_box(&info) {
                    Ok(data) => data,
                    Err(BoxReadError::Parse(e)) => {
                        ctx.emit_box_parse_error(&e);
                        continue;
                    }
                    Err(e @ BoxReadError::Io(_)) => return Err(e),
                };
                match RawBox::new(&data) {
                    Ok(raw_box) => {
                        ctx.push_path_indexed(raw_box.box_type(), sibling_index);
                        self.rules.validate_box(ctx, &raw_box);
                        ctx.pop_path();
                    }
                    Err(e) => {
                        ctx.push_path_indexed(info.box_type, sibling_index);
                        ctx.emit_box_parse_error(&e);
                        ctx.pop_path();
                    }
                }
            } else {
                ctx.push_path_indexed(info.box_type, sibling_index);
                ctx.emit(DiagnosticType::BoxTooLargeToValidate {
                    box_type: info.box_type.0,
                    size: iter.effective_size(&info),
                    limit: MAX_BOX_SIZE,
                });
                ctx.pop_path();
                iter.skip_box(&info)?;
            }
        }

        Ok(())
    }
}

impl Default for Validator<DefaultRules> {
    fn default() -> Self {
        Self::new()
    }
}

pub fn is_container_box(box_type: BoxCode) -> bool {
    matches!(
        box_type,
        BoxCode::MOOV
            | BoxCode::TRAK
            | BoxCode::MDIA
            | BoxCode::MINF
            | BoxCode::STBL
            | BoxCode::DINF
            | BoxCode::EDTS
            | BoxCode::MVEX
            | BoxCode::MOOF
            | BoxCode::TRAF
            | BoxCode::MFRA
            | BoxCode::UDTA
            | BoxCode::SINF
            | BoxCode::SCHI
            | BoxCode::TREF
            | BoxCode::IPRP
            | BoxCode::IPCO
            | BoxCode::GRPL
            | BoxCode::TRGR
            | BoxCode::META
    )
}

pub fn payload_skip_for(box_type: BoxCode) -> u64 {
    if matches!(box_type, BoxCode::META | BoxCode::DREF) {
        4
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn validator_creation() {
        let validator = Validator::new();
        assert!(validator.rule_count() > 0);
    }

    #[test]
    fn validate_empty_file() {
        let validator = Validator::new();
        let cursor = Cursor::new(Vec::<u8>::new());
        let report = validator.validate(cursor).unwrap();
        assert!(report.has_errors());
    }

    fn make_box(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let size = 8 + payload.len();
        let mut data = Vec::with_capacity(size);
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(box_type);
        data.extend_from_slice(payload);
        data
    }

    #[test]
    fn fuzz_crash_extended_size_overflow() {
        let data: Vec<u8> = vec![
            0, 0, 0, 18, 102, 116, 121, 112, 1, 197, 0, 213, 197, 61, 197, 197,
            199, 136, 0, 0, 0, 1, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 34, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 8,
        ];
        let validator = Validator::new();
        let cursor = Cursor::new(data);
        let _ = validator.validate(cursor);
    }

    #[test]
    fn fuzz_crash_moov_container() {
        let data: Vec<u8> = vec![
            0, 0, 0, 16, 115, 116, 115, 104, 109, 111, 111, 118, 0, 0, 0, 18,
            109, 111, 18, 102, 116, 121, 112, 1, 197, 0, 197, 197, 61, 197, 229,
            199, 136, 0, 0, 0, 18, 102, 111, 118, 253, 223, 115, 0, 109, 111,
            111, 118, 0, 0, 0, 128, 116, 114, 0, 0, 0, 0, 0, 0, 109, 225,
            0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 0, 0, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 49, 255, 255, 255, 255,
            255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
            255, 255, 255, 255, 0, 0, 116, 114, 3, 116, 116, 114, 101, 102, 0, 0, 0,
        ];
        let validator = Validator::new();
        let cursor = Cursor::new(data);
        let _ = validator.validate(cursor);
    }

    #[test]
    fn fuzz_crash_mfra_container() {
        let data: Vec<u8> = vec![
            0, 0, 0, 18, 116, 102, 104, 100, 1, 197, 0, 197, 197, 61, 197, 197,
            199, 136, 0, 0, 0, 18, 102, 116, 121, 112, 1, 197, 187, 0, 213, 61,
            1, 0, 0, 0, 0, 0, 0, 0, 102, 116, 121, 112, 159, 159, 247, 21,
            255, 1, 0, 0, 0, 0, 0, 0, 80, 255, 0, 159, 0, 0, 0, 16,
            0, 0, 100, 104, 107, 116, 0, 24, 109, 102, 114, 97, 247, 21, 255, 255,
            0, 0, 0, 0, 0, 0, 121, 112, 1, 197, 0, 159, 159, 159, 197, 197,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 254, 255, 0, 0, 0, 0, 0, 0,
        ];
        let validator = Validator::new();
        let cursor = Cursor::new(data);
        let _ = validator.validate(cursor);
    }

    #[test]
    fn fuzz_crash_stsc_overflow() {
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x00, 0x74, 0x72, 0x65, 0x66, 0x00, 0x00, 0x00, 0x00,
            0x74, 0x72, 0x65, 0x66, 0x00, 0x00, 0x00, 0x00, 0x73, 0x74, 0x73, 0x63,
            0x00, 0xdf, 0x72, 0x61, 0x00, 0x00, 0x00, 0x10, 0x95, 0x00, 0x00, 0x00,
            0x08, 0x01, 0x00, 0x00, 0x90, 0x21, 0x00, 0xff, 0xff, 0xff, 0xdc, 0xff,
            0xff, 0x3e, 0x74, 0x74, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x74, 0x74, 0xff, 0xff, 0x00,
            0xff, 0xff, 0x74, 0xff, 0xff, 0x00, 0xff, 0xff, 0x00, 0xff, 0xff, 0xff,
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x60, 0x6f, 0x6f, 0x76, 0x00, 0x00,
            0x00, 0x90, 0x00, 0x00, 0x00, 0x11, 0x00, 0xaa, 0x00, 0x00, 0x00, 0x00,
            0xff, 0x6e, 0x78, 0x6f, 0x6f, 0xff, 0xff, 0xff, 0xfe, 0xff, 0x00, 0x00,
            0x00, 0x00, 0x72, 0x65, 0x66, 0x00, 0x00, 0x00, 0x73, 0x74, 0x73, 0x63,
            0x00, 0xdf, 0x72, 0x61, 0x00, 0x00, 0x00, 0x10, 0x95, 0x00, 0x00, 0x00,
            0x08, 0x01, 0x00, 0x00, 0x90, 0x21, 0x00, 0xff, 0xff, 0xff, 0xdc, 0xff,
            0xff, 0x3e, 0x74, 0x74, 0x00, 0x00, 0x00, 0x08, 0x00, 0xaa, 0x00, 0x00,
            0x00, 0xff, 0x6e, 0x78, 0x6f, 0x6f, 0xff, 0xff, 0xff, 0xfe, 0xff, 0x00,
            0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x60,
            0x3a, 0x00, 0x6d, 0x6f, 0x6f, 0x76, 0x00, 0x00, 0x00, 0x90, 0x00, 0x00,
            0x00, 0x11, 0x00, 0xaa, 0x00, 0x00, 0x00, 0x00, 0xff, 0x6e, 0x78, 0x6f,
            0x6f, 0xff, 0xff, 0xff, 0xfe, 0xff, 0x00, 0x00,
        ];
        let validator = Validator::new();
        let cursor = Cursor::new(data);
        let report = validator.validate(cursor).unwrap();
        assert!(report.diagnostics().iter().any(|d| d.code() == "ST023"));
    }

    #[test]
    fn fuzz_crash_stz2_min_size() {
        let data: Vec<u8> = vec![
            0x00, 0x00, 0x00, 0x10, // size = 16
            0x73, 0x74, 0x7a, 0x32, // "stz2"
            0x00, 0x00, 0x00, 0x00, // version + flags
            0x00, 0x00, 0x00, 0x08, // reserved(3) + field_size(1)
        ];
        let validator = Validator::new();
        let cursor = Cursor::new(data);
        let _ = validator.validate(cursor);
    }

    #[test]
    fn validate_ftyp_only() {
        let validator = Validator::new();
        let ftyp = make_box(b"ftyp", b"isom\x00\x00\x02\x00");
        let cursor = Cursor::new(ftyp);
        let report = validator.validate(cursor).unwrap();
        assert!(report.has_errors());
        let messages: Vec<_> = report
            .diagnostics()
            .iter()
            .map(|d| d.message())
            .collect();
        assert!(!messages.iter().any(|m| m.contains("ftyp")));
        assert!(messages.iter().any(|m| m.contains("moov")));
    }

    #[test]
    fn two_trak_paths_disambiguated() {
        let trak1 = make_box(b"trak", &[]);
        let trak2 = make_box(b"trak", &[]);
        let mut moov_payload = Vec::new();
        moov_payload.extend_from_slice(&trak1);
        moov_payload.extend_from_slice(&trak2);
        let moov = make_box(b"moov", &moov_payload);

        let validator = Validator::new();
        let report = validator.validate_bytes(&moov);

        let text = report.format_text();

        assert!(
            text.contains("trak[0]"),
            "Expected trak[0] in output:\n{}",
            text
        );
        assert!(
            text.contains("trak[1]"),
            "Expected trak[1] in output:\n{}",
            text
        );

        assert!(
            !text.contains("moov["),
            "moov should not be indexed:\n{}",
            text
        );

        let json = report.format_json().unwrap();
        assert!(json.contains("trak[0]"));
        assert!(json.contains("trak[1]"));
        assert!(!json.contains("moov["));
    }

    #[test]
    fn two_trak_paths_disambiguated_streaming() {
        let trak1 = make_box(b"trak", &[]);
        let trak2 = make_box(b"trak", &[]);
        let mut moov_payload = Vec::new();
        moov_payload.extend_from_slice(&trak1);
        moov_payload.extend_from_slice(&trak2);
        let moov = make_box(b"moov", &moov_payload);

        let validator = Validator::new();
        let cursor = Cursor::new(moov);
        let report = validator.validate(cursor).unwrap();

        let text = report.format_text();
        assert!(text.contains("trak[0]"));
        assert!(text.contains("trak[1]"));
        assert!(!text.contains("moov["));
    }
}
