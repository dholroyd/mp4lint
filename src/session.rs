
use crate::context::{ContainerChildTracker, ValidationContext, ValidationOptions};
use crate::diagnostic::DiagnosticType;
use crate::report::ValidationReport;
use crate::rules::{DefaultRules, RuleSet};
use crate::validator::{is_container_box, payload_skip_for, MAX_BOX_SIZE};
use isobmff_syntax::{BoxInfo, RawBox, SliceBoxIterator};

pub struct ValidationSession<R: RuleSet = DefaultRules> {
    rules: R,
    ctx: ValidationContext,
}

impl ValidationSession<DefaultRules> {
    pub fn new(file_size: u64, options: ValidationOptions) -> Self {
        let mut ctx = ValidationContext::new(options);
        ctx.state_mut().file_size = file_size;
        Self {
            rules: DefaultRules,
            ctx,
        }
    }
}

impl<R: RuleSet> ValidationSession<R> {
    pub fn with_rules(rules: R, file_size: u64, options: ValidationOptions) -> Self {
        let mut ctx = ValidationContext::new(options);
        ctx.state_mut().file_size = file_size;
        Self { rules, ctx }
    }

    pub fn observe_box(&mut self, info: &BoxInfo) {
        self.ctx
            .state_mut()
            .top_level_tracker
            .record_child(info.box_type);
        self.rules.observe_child(&mut self.ctx, info);
        self.rules.observe_box(&mut self.ctx, info);
    }

    pub fn add_box(&mut self, data: &[u8], file_offset: u64) {
        let mut iter = match SliceBoxIterator::with_base_offset(data, file_offset) {
            Ok(iter) => iter,
            Err(e) => {
                self.ctx.emit_box_parse_error(&e);
                return;
            }
        };

        let info = match iter.next_info() {
            Ok(Some(info)) => info,
            Ok(None) => return,
            Err(e) => {
                self.ctx.emit_box_parse_error(&e);
                return;
            }
        };

        {
            let tl = &mut self.ctx.state_mut().top_level_tracker;
            tl.record_child(info.box_type);
        }
        let sibling_index = self.ctx.state().top_level_tracker.child_count(info.box_type) - 1;

        self.rules.observe_child(&mut self.ctx, &info);
        self.rules.observe_box(&mut self.ctx, &info);

        if is_container_box(info.box_type) {
            let box_type = info.box_type;

            self.ctx.push_path_indexed(box_type, sibling_index);

            let tracker = ContainerChildTracker {
                box_info_size: info.size,
                ..Default::default()
            };
            self.ctx
                .state_mut()
                .container_stack
                .push((box_type, tracker));

            self.rules.enter_container(&mut self.ctx, box_type, &info);

            let payload_skip = payload_skip_for(box_type);
            if let Err(e) = iter.enter_container(&info, payload_skip) {
                self.ctx.emit_box_parse_error(&e);
            } else {
                self.walk_slice(&mut iter);
                iter.exit_container();
            }

            self.rules.exit_container(&mut self.ctx, box_type);

            let container_path = self.ctx.path().to_string();
            let child_counts = self
                .ctx
                .state()
                .container_stack
                .last()
                .map(|(_, tracker)| tracker.child_counts.clone());
            if let Some(counts) = child_counts {
                self.ctx
                    .state_mut()
                    .sibling_counts
                    .record(&container_path, counts);
            }

            self.ctx.state_mut().container_stack.pop();
            self.ctx.pop_path();
        } else if iter.effective_size(&info) <= MAX_BOX_SIZE {
            let box_data = iter.load_box(&info);
            match RawBox::new(box_data) {
                Ok(raw_box) => {
                    self.ctx.push_path_indexed(raw_box.box_type(), sibling_index);
                    self.rules.validate_box(&mut self.ctx, &raw_box);
                    self.ctx.pop_path();
                }
                Err(e) => {
                    self.ctx.push_path_indexed(info.box_type, sibling_index);
                    self.ctx.emit_box_parse_error(&e);
                    self.ctx.pop_path();
                }
            }
        } else {
            self.ctx.push_path_indexed(info.box_type, sibling_index);
            self.ctx.emit(DiagnosticType::BoxTooLargeToValidate {
                box_type: info.box_type.0,
                size: iter.effective_size(&info),
                limit: MAX_BOX_SIZE,
            });
            self.ctx.pop_path();
        }
    }

    pub fn finalize(mut self) -> ValidationReport {
        self.rules.finalize(&mut self.ctx);
        let (diagnostics, sibling_counts) = self.ctx.into_diagnostics_and_sibling_counts();
        ValidationReport::from_diagnostics(diagnostics, sibling_counts)
    }

    fn walk_slice(&mut self, iter: &mut SliceBoxIterator<'_>) {
        loop {
            let info = match iter.next_info() {
                Ok(Some(info)) => info,
                Ok(None) => break,
                Err(e) => {
                    self.ctx.emit_box_parse_error(&e);
                    break;
                }
            };

            self.rules.observe_child(&mut self.ctx, &info);

            let sibling_index =
                if let Some((_, tracker)) = self.ctx.state_mut().container_stack.last_mut() {
                    tracker.record_child(info.box_type);
                    tracker.child_count(info.box_type) - 1
                } else {
                    let tl = &mut self.ctx.state_mut().top_level_tracker;
                    tl.record_child(info.box_type);
                    tl.child_count(info.box_type) - 1
                };

            if is_container_box(info.box_type) {
                let box_type = info.box_type;

                self.ctx.push_path_indexed(box_type, sibling_index);

                let tracker = ContainerChildTracker {
                    box_info_size: info.size,
                    ..Default::default()
                };
                self.ctx
                    .state_mut()
                    .container_stack
                    .push((box_type, tracker));

                self.rules.enter_container(&mut self.ctx, box_type, &info);

                let payload_skip = payload_skip_for(box_type);
                if let Err(e) = iter.enter_container(&info, payload_skip) {
                    self.ctx.emit_box_parse_error(&e);
                    iter.skip_box(&info);
                } else {
                    self.walk_slice(iter);
                    iter.exit_container();
                }

                self.rules.exit_container(&mut self.ctx, box_type);

                let container_path = self.ctx.path().to_string();
                let child_counts = self
                    .ctx
                    .state()
                    .container_stack
                    .last()
                    .map(|(_, tracker)| tracker.child_counts.clone());
                if let Some(counts) = child_counts {
                    self.ctx
                        .state_mut()
                        .sibling_counts
                        .record(&container_path, counts);
                }

                self.ctx.state_mut().container_stack.pop();
                self.ctx.pop_path();
            } else if iter.effective_size(&info) <= MAX_BOX_SIZE {
                let box_data = iter.load_box(&info);
                match RawBox::new(box_data) {
                    Ok(raw_box) => {
                        self.ctx
                            .push_path_indexed(raw_box.box_type(), sibling_index);
                        self.rules.validate_box(&mut self.ctx, &raw_box);
                        self.ctx.pop_path();
                    }
                    Err(e) => {
                        self.ctx.push_path_indexed(info.box_type, sibling_index);
                        self.ctx.emit_box_parse_error(&e);
                        self.ctx.pop_path();
                    }
                }
            } else {
                self.ctx.push_path_indexed(info.box_type, sibling_index);
                self.ctx.emit(DiagnosticType::BoxTooLargeToValidate {
                    box_type: info.box_type.0,
                    size: iter.effective_size(&info),
                    limit: MAX_BOX_SIZE,
                });
                self.ctx.pop_path();
                iter.skip_box(&info);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use isobmff_syntax::BoxCode;

    fn make_box(box_type: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let size = 8 + payload.len();
        let mut data = Vec::with_capacity(size);
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(box_type);
        data.extend_from_slice(payload);
        data
    }

    #[test]
    fn session_empty_file() {
        let session = ValidationSession::new(0, ValidationOptions::default());
        let report = session.finalize();
        assert!(report.has_errors());
        let codes: Vec<_> = report.diagnostics().iter().map(|d| d.code()).collect();
        assert!(codes.contains(&"E001")); // ftyp missing
        assert!(codes.contains(&"E003")); // moov missing
    }

    #[test]
    fn session_ftyp_only() {
        let ftyp = make_box(b"ftyp", b"isom\x00\x00\x02\x00");
        let file_size = ftyp.len() as u64;

        let mut session = ValidationSession::new(file_size, ValidationOptions::default());
        session.add_box(&ftyp, 0);
        let report = session.finalize();

        assert!(report.has_errors());
        let messages: Vec<_> = report.diagnostics().iter().map(|d| d.message()).collect();
        assert!(!messages.iter().any(|m| m.contains("ftyp")));
        assert!(messages.iter().any(|m| m.contains("moov")));
    }

    #[test]
    fn session_matches_validator_for_ftyp() {
        let ftyp = make_box(b"ftyp", b"isom\x00\x00\x02\x00");
        let file_size = ftyp.len() as u64;

        let validator = crate::Validator::new();
        let validator_report = validator.validate_bytes(&ftyp);

        let mut session = ValidationSession::new(file_size, ValidationOptions::default());
        session.add_box(&ftyp, 0);
        let session_report = session.finalize();

        let mut validator_codes: Vec<_> = validator_report
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        let mut session_codes: Vec<_> = session_report
            .diagnostics()
            .iter()
            .map(|d| d.code().to_string())
            .collect();
        validator_codes.sort();
        session_codes.sort();
        assert_eq!(validator_codes, session_codes);
    }

    #[test]
    fn session_observe_box_tracks_mdat() {
        let ftyp = make_box(b"ftyp", b"isom\x00\x00\x02\x00");
        let ftyp_len = ftyp.len() as u64;
        let mdat_offset = ftyp_len;
        let mdat_size: u64 = 1000;
        let file_size = ftyp_len + mdat_size;

        let mut session = ValidationSession::new(file_size, ValidationOptions::default());
        session.add_box(&ftyp, 0);
        session.observe_box(&BoxInfo {
            box_type: BoxCode::MDAT,
            offset: mdat_offset,
            size: isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(mdat_size).unwrap()),
            header_size: 8,
        });
        let _report = session.finalize();
    }

    #[test]
    fn session_observe_box_zero_sized_not_last_emits_f010() {
        let ftyp = make_box(b"ftyp", b"isom\x00\x00\x02\x00");
        let ftyp_len = ftyp.len() as u64;
        let file_size = ftyp_len + 1000;

        let mut session = ValidationSession::new(file_size, ValidationOptions::default());
        session.add_box(&ftyp, 0);

        session.observe_box(&BoxInfo {
            box_type: BoxCode::MDAT,
            offset: ftyp_len,
            size: isobmff_syntax::BoxSize::ToEnd,
            header_size: 8,
        });

        session.observe_box(&BoxInfo {
            box_type: BoxCode::FREE,
            offset: ftyp_len + 500,
            size: isobmff_syntax::BoxSize::Known(std::num::NonZeroU64::new(100).unwrap()),
            header_size: 8,
        });

        let report = session.finalize();
        let codes: Vec<_> = report.diagnostics().iter().map(|d| d.code()).collect();
        assert!(codes.contains(&"F010"), "expected F010, got: {:?}", codes);
    }

    #[test]
    fn session_with_nonzero_file_offset() {
        let ftyp = make_box(b"ftyp", b"isom\x00\x00\x02\x00");
        let file_offset: u64 = 100;
        let file_size = file_offset + ftyp.len() as u64;

        let mut session = ValidationSession::new(file_size, ValidationOptions::default());
        session.add_box(&ftyp, file_offset);
        let report = session.finalize();

        let codes: Vec<_> = report.diagnostics().iter().map(|d| d.code()).collect();
        assert!(!codes.contains(&"E001"));
    }
}
