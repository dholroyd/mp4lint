
use crate::context::ValidationContext;
use crate::diagnostic::{DiagnosticType, PropertyPath};
use crate::rules::ValidationRule;
use isobmff_syntax::boxes::{
    DataReferenceBox, DataReferenceBoxView,
    ExtendedLanguageTagBox, ExtendedLanguageTagBoxView,
    HandlerReferenceBox, HandlerReferenceBoxView,
    MediaHeaderBox, MediaHeaderBoxView,
};
use isobmff_syntax::{BoxCode, HandlerCode, RawBox};

pub struct MinfMediaHeaderRule;

impl ValidationRule for MinfMediaHeaderRule {
    fn code(&self) -> &'static str {
        "MD005"
    }

    fn name(&self) -> &'static str {
        "minf-media-header"
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::MINF {
            return;
        }

        let has_any = ctx.current_container_tracker()
            .map(|t| {
                t.has_child(BoxCode::VMHD)
                    || t.has_child(BoxCode::SMHD)
                    || t.has_child(BoxCode::HMHD)
                    || t.has_child(BoxCode::NMHD)
                    || t.has_child(BoxCode::STHD)
            })
            .unwrap_or(true);

        if !has_any {
            ctx.emit(DiagnosticType::MinfMissingMediaHeader);
        }
    }
}

pub struct DinfRequiredRule;

impl ValidationRule for DinfRequiredRule {
    fn code(&self) -> &'static str {
        "MD007"
    }

    fn name(&self) -> &'static str {
        "dinf-required"
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::MINF {
            return;
        }

        let has_dinf = ctx.current_container_tracker()
            .map(|t| t.has_child(BoxCode::DINF))
            .unwrap_or(true);

        if !has_dinf {
            ctx.emit(DiagnosticType::MinfMissingDinf);
        }
    }
}

pub struct DrefRequiredRule;

impl ValidationRule for DrefRequiredRule {
    fn code(&self) -> &'static str {
        "MD008"
    }

    fn name(&self) -> &'static str {
        "dref-required"
    }

    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
        if box_type != BoxCode::DINF {
            return;
        }

        let has_dref = ctx.current_container_tracker()
            .map(|t| t.has_child(BoxCode::DREF))
            .unwrap_or(true);

        if !has_dref {
            ctx.emit(DiagnosticType::DinfMissingDref);
        }
    }
}

pub struct DrefEntryCountRule;

impl ValidationRule for DrefEntryCountRule {
    fn code(&self) -> &'static str {
        "MD009"
    }

    fn name(&self) -> &'static str {
        "dref-entry-count"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::DREF {
            return;
        }

        let Ok(dref) = DataReferenceBoxView::new(raw_box.data()) else {
            return;
        };

        if dref.entry_count() < 1 {
            ctx.emit_property(DiagnosticType::DrefEmpty, PropertyPath::from("entry_count"));
        }
    }
}

pub struct MdhdLanguageValidRule;

impl ValidationRule for MdhdLanguageValidRule {
    fn code(&self) -> &'static str {
        "MD010"
    }

    fn name(&self) -> &'static str {
        "mdhd-language-valid"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::MDHD {
            return;
        }

        let Ok(mdhd) = MediaHeaderBoxView::new(raw_box.data()) else {
            return;
        };

        if !mdhd.language().is_valid() {
            ctx.emit_property(DiagnosticType::MdhdLanguageInvalid, PropertyPath::from("language"));
        }
    }
}

pub struct ElngValidBcp47Rule;

impl ValidationRule for ElngValidBcp47Rule {
    fn code(&self) -> &'static str {
        "MD011"
    }

    fn name(&self) -> &'static str {
        "elng-valid-bcp47"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::ELNG {
            return;
        }

        let Ok(elng) = ExtendedLanguageTagBoxView::new(raw_box.data()) else {
            return;
        };

        let lang = elng.extended_language();
        if lang.is_empty() {
            ctx.emit_property(DiagnosticType::ElngEmpty, PropertyPath::from("extended_language"));
            return;
        }
        let s = std::str::from_utf8(lang).unwrap_or("");
        if s.is_empty()
            || !s
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            ctx.emit_property(DiagnosticType::ElngInvalidBcp47, PropertyPath::from("extended_language"));
        }
    }
}

pub struct HandlerTypeKnownRule;

impl ValidationRule for HandlerTypeKnownRule {
    fn code(&self) -> &'static str {
        "MD012"
    }

    fn name(&self) -> &'static str {
        "handler-type-known"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::HDLR {
            return;
        }
        if !ctx.path().contains(BoxCode::MDIA) {
            return;
        }

        let Ok(hdlr) = HandlerReferenceBoxView::new(raw_box.data()) else {
            return;
        };

        let ht = hdlr.handler_type();
        let known: &[HandlerCode] = &[
            HandlerCode::VIDE, HandlerCode::SOUN, HandlerCode::HINT,
            HandlerCode::META, HandlerCode::AUXV, HandlerCode::TEXT,
            HandlerCode::SUBT, HandlerCode::FDSM, HandlerCode::new(*b"font"),
            HandlerCode::TMCD, HandlerCode::HAPT, HandlerCode::new(*b"volu"),
        ];
        if !known.contains(&ht) {
            ctx.emit_property(DiagnosticType::UnknownHandlerType { handler: ht.0 }, PropertyPath::from("handler_type"));
        }
    }
}

pub struct HdlrNameNullTermRule;

impl ValidationRule for HdlrNameNullTermRule {
    fn code(&self) -> &'static str {
        "MD013"
    }

    fn name(&self) -> &'static str {
        "hdlr-name-null-term"
    }

    fn is_critical(&self) -> bool {
        false
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::HDLR {
            return;
        }

        let data = raw_box.data();
        let Ok(hdlr) = HandlerReferenceBoxView::new(data) else {
            return;
        };
        let _ = hdlr; // we only used it for parse validation
        if data.is_empty() || data[data.len() - 1] != 0x00 {
            ctx.emit_property(DiagnosticType::HdlrNameNotNullTerminated, PropertyPath::from("name"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ValidationOptions;

    fn make_hdlr(handler_type: &[u8; 4]) -> Vec<u8> {
        let mut data = Vec::new();
        let size = 8 + 4 + 4 + 4 + 12 + 1;
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"hdlr");
        data.extend_from_slice(&[0, 0, 0, 0]); // version + flags
        data.extend_from_slice(&0u32.to_be_bytes()); // pre_defined
        data.extend_from_slice(handler_type);
        data.extend_from_slice(&[0u8; 12]); // reserved
        data.push(0); // null name
        data
    }

    fn make_mdhd(timescale: u32, language_raw: u16) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&32u32.to_be_bytes());
        data.extend_from_slice(b"mdhd");
        data.push(0);
        data.extend_from_slice(&[0, 0, 0]);
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&timescale.to_be_bytes());
        data.extend_from_slice(&0u32.to_be_bytes());
        data.extend_from_slice(&language_raw.to_be_bytes());
        data.extend_from_slice(&0u16.to_be_bytes());
        data
    }

    fn make_dref(entry_count: u32) -> Vec<u8> {
        let mut data = Vec::new();
        let size = 8 + 4 + 4;
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"dref");
        data.extend_from_slice(&[0, 0, 0, 0]); // version + flags
        data.extend_from_slice(&entry_count.to_be_bytes());
        data
    }

    #[test]
    fn md005_minf_with_vmhd() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = crate::context::ContainerChildTracker::default();
        tracker.record_child(BoxCode::VMHD);
        tracker.record_child(BoxCode::STBL);
        ctx.state_mut().container_stack.push((BoxCode::MINF, tracker));
        MinfMediaHeaderRule.exit_container(&mut ctx, BoxCode::MINF);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn md005_minf_missing_header() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut tracker = crate::context::ContainerChildTracker::default();
        tracker.record_child(BoxCode::STBL);
        ctx.state_mut().container_stack.push((BoxCode::MINF, tracker));
        MinfMediaHeaderRule.exit_container(&mut ctx, BoxCode::MINF);
        assert!(ctx.has_errors());
    }

    #[test]
    fn md009_dref_valid() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let dref = make_dref(1);
        let raw = RawBox::new(&dref).unwrap();
        DrefEntryCountRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn md009_dref_zero_entries() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let dref = make_dref(0);
        let raw = RawBox::new(&dref).unwrap();
        DrefEntryCountRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }

    #[test]
    fn md010_valid_language() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mdhd = make_mdhd(48000, 0x55C4);
        let raw = RawBox::new(&mdhd).unwrap();
        MdhdLanguageValidRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn md010_invalid_language() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mdhd = make_mdhd(48000, 0x0000);
        let raw = RawBox::new(&mdhd).unwrap();
        MdhdLanguageValidRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn md012_known_handler() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.push_path(BoxCode::MOOV);
        ctx.push_path(BoxCode::TRAK);
        ctx.push_path(BoxCode::MDIA);
        let hdlr = make_hdlr(b"vide");
        let raw = RawBox::new(&hdlr).unwrap();
        HandlerTypeKnownRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn md012_unknown_handler() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        ctx.push_path(BoxCode::MOOV);
        ctx.push_path(BoxCode::TRAK);
        ctx.push_path(BoxCode::MDIA);
        let hdlr = make_hdlr(b"xxxx");
        let raw = RawBox::new(&hdlr).unwrap();
        HandlerTypeKnownRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
    }

    #[test]
    fn md013_name_null_terminated() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let hdlr = make_hdlr(b"vide");
        let raw = RawBox::new(&hdlr).unwrap();
        HdlrNameNullTermRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 0);
    }

    #[test]
    fn md013_name_not_null_terminated() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let mut data = Vec::new();
        let size = 8 + 4 + 4 + 4 + 12 + 5; // "Video" without null
        data.extend_from_slice(&(size as u32).to_be_bytes());
        data.extend_from_slice(b"hdlr");
        data.extend_from_slice(&[0, 0, 0, 0]); // version + flags
        data.extend_from_slice(&0u32.to_be_bytes()); // pre_defined
        data.extend_from_slice(b"vide");
        data.extend_from_slice(&[0u8; 12]); // reserved
        data.extend_from_slice(b"Video"); // name without null terminator
        let raw = RawBox::new(&data).unwrap();
        HdlrNameNullTermRule.validate_box(&mut ctx, &raw);
        assert_eq!(ctx.warning_count(), 1);
    }
}
