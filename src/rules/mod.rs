
use crate::context::ValidationContext;
use isobmff_syntax::{BoxCode, BoxInfo, RawBox};

pub mod accumulator;
pub mod brand;
pub mod cross_box;
pub mod field_constraints;
pub mod fragment;
pub mod item_properties;
pub mod media;
pub mod sample_table;
pub mod structural;
pub mod timing;
pub mod track;
pub mod track_ref;
pub mod visual_sample;

pub trait ValidationRule: Send + Sync {
    fn code(&self) -> &'static str;

    fn name(&self) -> &'static str;

    fn is_critical(&self) -> bool {
        true
    }

    fn observe_box(&self, _ctx: &mut ValidationContext, _info: &BoxInfo) {}

    fn observe_child(&self, _ctx: &mut ValidationContext, _info: &BoxInfo) {}

    fn enter_container(&self, _ctx: &mut ValidationContext, _box_type: BoxCode, _info: &BoxInfo) {}

    fn validate_box(&self, _ctx: &mut ValidationContext, _raw_box: &RawBox<'_>) {}

    fn exit_container(&self, _ctx: &mut ValidationContext, _box_type: BoxCode) {}

    fn finalize(&self, _ctx: &mut ValidationContext) {}
}

pub trait RuleSet: Send + Sync {
    fn observe_box(&self, ctx: &mut ValidationContext, info: &BoxInfo);
    fn observe_child(&self, ctx: &mut ValidationContext, info: &BoxInfo);
    fn enter_container(&self, ctx: &mut ValidationContext, box_type: BoxCode, info: &BoxInfo);
    fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>);
    fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode);
    fn finalize(&self, ctx: &mut ValidationContext);
    fn rule_count(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.rule_count() == 0
    }
}

pub struct DefaultRules;

macro_rules! default_rules {
    ($($rule:expr),* $(,)?) => {
        impl RuleSet for DefaultRules {
            fn observe_box(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
                let lenient = ctx.options().lenient;
                $( { let r = $rule; if !lenient || r.is_critical() { r.observe_box(ctx, info); } } )*
            }
            fn observe_child(&self, ctx: &mut ValidationContext, info: &BoxInfo) {
                let lenient = ctx.options().lenient;
                $( { let r = $rule; if !lenient || r.is_critical() { r.observe_child(ctx, info); } } )*
            }
            fn enter_container(&self, ctx: &mut ValidationContext, box_type: BoxCode, info: &BoxInfo) {
                let lenient = ctx.options().lenient;
                $( { let r = $rule; if !lenient || r.is_critical() { r.enter_container(ctx, box_type, info); } } )*
            }
            fn validate_box(&self, ctx: &mut ValidationContext, raw_box: &RawBox<'_>) {
                let lenient = ctx.options().lenient;
                $( { let r = $rule; if !lenient || r.is_critical() { r.validate_box(ctx, raw_box); } } )*
            }
            fn exit_container(&self, ctx: &mut ValidationContext, box_type: BoxCode) {
                let lenient = ctx.options().lenient;
                $( { let r = $rule; if !lenient || r.is_critical() { r.exit_container(ctx, box_type); } } )*
            }
            fn finalize(&self, ctx: &mut ValidationContext) {
                let lenient = ctx.options().lenient;
                $( { let r = $rule; if !lenient || r.is_critical() { r.finalize(ctx); } } )*
            }
            fn rule_count(&self) -> usize {
                let mut count = 0usize;
                $( { let _ = $rule; count += 1; } )*
                count
            }
        }
    };
}

default_rules![
    accumulator::StateAccumulatorRule,
    structural::FtypPresenceRule,
    structural::FtypPositionRule,
    structural::MoovUniquenessRule,
    structural::BoxHierarchyRule,
    structural::BoxSizeValidityRule,
    structural::BoxTypePrintableAsciiRule,
    structural::ReservedBoxTypesRule,
    structural::TopLevelBoxTypesRule,
    structural::ZeroSizedBoxNotLastRule,
    track::MediaTrackPresenceRule,
    track::TrackIdUniquenessRule,
    track::TkhdVersionRule,
    track::TkhdFlagsRule,
    track::TrefUniquenessRule,
    track::TrefNoDuplicateTypesRule,
    sample_table::StblRequiredBoxesRule,
    sample_table::ChunkOffsetExclusivityRule,
    sample_table::SampleCountConsistencyRule,
    sample_table::StsdEntryCountRule,
    sample_table::SttsPositiveDeltaRule,
    sample_table::StscValidationRule,
    sample_table::CttsCountConsistencyRule,
    sample_table::CttsAllZeroOffsetsRule,
    sample_table::UniqueCompositionTimestampsRule,
    sample_table::StssStrictlyIncreasingRule,
    sample_table::StssValidRangeRule,
    sample_table::StshSortedRule,
    sample_table::Stz2FieldSizeRule,
    sample_table::SampleEntryDataRefIndexRule,
    timing::TimescaleNonzeroRule,
    timing::MvhdNextTrackIdNonzeroRule,
    timing::MvhdNextTrackIdGreaterRule,
    timing::MvhdVersionRule,
    timing::MvhdDurationConsistencyRule,
    timing::TrackDurationConsistencyRule,
    timing::EditListDurationRule,
    timing::EditListMediaTimeRule,
    timing::EditListMediaRateRule,
    timing::IndefiniteDurationRule,
    timing::PtsStartEditListRule,
    media::MinfMediaHeaderRule,
    media::DinfRequiredRule,
    media::DrefRequiredRule,
    media::DrefEntryCountRule,
    media::MdhdLanguageValidRule,
    media::ElngValidBcp47Rule,
    media::HandlerTypeKnownRule,
    media::HdlrNameNullTermRule,
    fragment::MoofMfhdRequiredRule,
    fragment::SequenceNumberOrderRule,
    fragment::MoofRequiresMvexRule,
    fragment::TrexPerTrackRule,
    fragment::TrexTrackIdValidRule,
    fragment::TrafTfhdRequiredRule,
    fragment::TfhdTrackIdValidRule,
    fragment::TfdtOrderRule,
    fragment::FragmentDurationRule,
    fragment::DurationIsEmptyWithEdtsRule,
    fragment::BaseDataOffsetConsistencyRule,
    fragment::SampleFlagsDefinedBitsRule,
    fragment::MfraPositionRule,
    fragment::MfroRequiredRule,
    fragment::DefaultBaseIsMoofBrandRule,
    track_ref::ReferenceTypeDefinedRule,
    track_ref::NoDuplicateTrackIdsInRefRule,
    track_ref::NoZeroTrackIdsInRefRule,
    track_ref::ReferencedTrackIdsExistRule,
    track_ref::HintTrackReferenceRule,
    track_ref::ReservedTrackRefTypesRule,
    field_constraints::FullBoxVersionRule,
    field_constraints::ReservedFieldsZeroRule,
    field_constraints::MatrixValuesRule,
    field_constraints::VolumeFieldRule,
    field_constraints::NonVisualTrackDimensionsRule,
    field_constraints::UnusedFlagBitsRule,
    field_constraints::PreDefinedFieldsZeroRule,
    cross_box::ChunkOffsetsWithinMdatRule,
    cross_box::ChunkDataExtentRule,
    cross_box::ChunkOffsetMonotonicityRule,
    cross_box::SaizSaioPairingRule,
    cross_box::TrackGroupIdConsistencyRule,
    cross_box::EntityGroupReferencesRule,
    cross_box::IlocReferencesRule,
    cross_box::PitmItemIdRule,
    visual_sample::PaspSpacingPositiveRule,
    item_properties::IinfEntryOrderingRule,
    item_properties::IpmaItemIdOrderingRule,
    item_properties::IpmaItemIdUniquenessRule,
    item_properties::IpmaVersionFlagsUniquenessRule,
    item_properties::IpmaVersionRecommendationRule,
    item_properties::IpmaFlagsRecommendationRule,
    brand::BrandComplianceRule,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rules_has_rules() {
        let rules = DefaultRules;
        assert!(!rules.is_empty());
    }
}
