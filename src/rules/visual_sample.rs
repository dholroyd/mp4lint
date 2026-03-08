
use crate::context::ValidationContext;
use crate::diagnostic::{DiagnosticType, PropertyPath};
use crate::rules::ValidationRule;
use isobmff_syntax::boxes::{PixelAspectRatioBox, PixelAspectRatioBoxView};
use isobmff_syntax::{BoxCode, RawBox};

pub struct PaspSpacingPositiveRule;

impl ValidationRule for PaspSpacingPositiveRule {
    fn code(&self) -> &'static str {
        "VS001"
    }

    fn name(&self) -> &'static str {
        "pasp-spacing-positive"
    }

    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
        if raw_box.box_type() != BoxCode::PASP {
            return;
        }

        let Ok(pasp) = PixelAspectRatioBoxView::new(raw_box.data()) else {
            return;
        };

        if pasp.h_spacing() == 0 {
            ctx.emit_property(DiagnosticType::PaspSpacingZero, PropertyPath::from("h_spacing"));
        }
        if pasp.v_spacing() == 0 {
            ctx.emit_property(DiagnosticType::PaspSpacingZero, PropertyPath::from("v_spacing"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ValidationOptions;

    fn make_pasp(h_spacing: u32, v_spacing: u32) -> Vec<u8> {
        let mut data = Vec::new();
        let size: u32 = 8 + 4 + 4;
        data.extend_from_slice(&size.to_be_bytes());
        data.extend_from_slice(b"pasp");
        data.extend_from_slice(&h_spacing.to_be_bytes());
        data.extend_from_slice(&v_spacing.to_be_bytes());
        data
    }

    #[test]
    fn vs001_valid_spacing() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let pasp = make_pasp(1, 1);
        let raw = RawBox::new(&pasp).unwrap();
        PaspSpacingPositiveRule.validate_box(&mut ctx, &raw);
        assert!(!ctx.has_errors());
    }

    #[test]
    fn vs001_zero_h_spacing() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let pasp = make_pasp(0, 1);
        let raw = RawBox::new(&pasp).unwrap();
        PaspSpacingPositiveRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "VS001");
    }

    #[test]
    fn vs001_zero_v_spacing() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let pasp = make_pasp(1, 0);
        let raw = RawBox::new(&pasp).unwrap();
        PaspSpacingPositiveRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
        assert_eq!(ctx.diagnostics()[0].code(), "VS001");
    }

    #[test]
    fn vs001_both_zero() {
        let mut ctx = ValidationContext::new(ValidationOptions::default());
        let pasp = make_pasp(0, 0);
        let raw = RawBox::new(&pasp).unwrap();
        PaspSpacingPositiveRule.validate_box(&mut ctx, &raw);
        assert!(ctx.has_errors());
    }
}
