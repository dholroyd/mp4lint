
use crate::diagnostic::{Diagnostic, DiagnosticLocation, Severity, SiblingCounts};
use serde::ser::SerializeStruct;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct ValidationReport {
    diagnostics: Vec<Diagnostic>,
    sibling_counts: SiblingCounts,
    error_count: usize,
    warning_count: usize,
    info_count: usize,
}

impl Serialize for ValidationReport {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        struct DiagWithCounts<'a> {
            diag: &'a Diagnostic,
            sibling_counts: &'a SiblingCounts,
        }

        impl<'a> Serialize for DiagWithCounts<'a> {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let has_location = self.diag.location.is_some();
                let field_count = if has_location { 4 } else { 3 };
                let mut st = serializer.serialize_struct("Diagnostic", field_count)?;
                st.serialize_field("severity", &self.diag.severity())?;
                st.serialize_field("code", self.diag.code())?;
                st.serialize_field("message", &self.diag.diagnostic_type.to_string())?;
                if let Some(ref loc) = self.diag.location {
                    let formatted = match loc {
                        DiagnosticLocation::Box(path) => self.sibling_counts.format_path(path),
                        DiagnosticLocation::Property(path, prop) => {
                            format!("{}.{}", self.sibling_counts.format_path(path), prop)
                        }
                    };
                    st.serialize_field("location", &formatted)?;
                }
                st.end()
            }
        }

        let entries: Vec<_> = self
            .diagnostics
            .iter()
            .map(|d| DiagWithCounts {
                diag: d,
                sibling_counts: &self.sibling_counts,
            })
            .collect();

        let mut st = serializer.serialize_struct("ValidationReport", 1)?;
        st.serialize_field("diagnostics", &entries)?;
        st.end()
    }
}

impl ValidationReport {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            sibling_counts: SiblingCounts::default(),
            error_count: 0,
            warning_count: 0,
            info_count: 0,
        }
    }

    pub fn from_diagnostics(diagnostics: Vec<Diagnostic>, sibling_counts: SiblingCounts) -> Self {
        let mut report = Self {
            sibling_counts,
            ..Self::new()
        };
        for diag in diagnostics {
            report.add(diag);
        }
        report
    }

    pub fn add(&mut self, diagnostic: Diagnostic) {
        match diagnostic.severity() {
            Severity::Error => self.error_count += 1,
            Severity::Warning => self.warning_count += 1,
            Severity::Info => self.info_count += 1,
        }
        self.diagnostics.push(diagnostic);
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn diagnostics_with_min_severity(&self, min: Severity) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(move |d| d.severity() >= min)
    }

    pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.severity() == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
        self.diagnostics.iter().filter(|d| d.severity() == Severity::Warning)
    }

    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    pub fn has_warnings(&self) -> bool {
        self.warning_count > 0
    }

    pub fn error_count(&self) -> usize {
        self.error_count
    }

    pub fn warning_count(&self) -> usize {
        self.warning_count
    }

    pub fn info_count(&self) -> usize {
        self.info_count
    }

    pub fn total_count(&self) -> usize {
        self.diagnostics.len()
    }

    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }

    pub fn format_text(&self) -> String {
        use std::fmt::Write;
        let mut output = String::new();

        for diag in &self.diagnostics {
            write!(
                output,
                "[{}] {}: {}",
                diag.code(),
                diag.severity(),
                diag.diagnostic_type
            )
            .unwrap();
            if let Some(ref loc) = diag.location {
                if !loc.is_empty() {
                    let formatted = match loc {
                        DiagnosticLocation::Box(path) => self.sibling_counts.format_path(path),
                        DiagnosticLocation::Property(path, prop) => {
                            format!("{}.{}", self.sibling_counts.format_path(path), prop)
                        }
                    };
                    write!(output, " at {}", formatted).unwrap();
                }
            }
            output.push('\n');
        }

        if self.has_errors() {
            output.push_str(&format!(
                "\nFound {} error(s) and {} warning(s)\n",
                self.error_count, self.warning_count
            ));
        } else if self.has_warnings() {
            output.push_str(&format!("\nFound {} warning(s)\n", self.warning_count));
        } else {
            output.push_str("\nNo errors found\n");
        }

        output
    }

    pub fn format_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

impl Default for ValidationReport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_counts() {
        use crate::diagnostic::DiagnosticType;
        use isobmff_syntax::FourCC;

        let mut report = ValidationReport::new();
        report.add(Diagnostic::new(DiagnosticType::FtypMissing));
        report.add(Diagnostic::new(DiagnosticType::MoovMissing));
        report.add(Diagnostic::new(DiagnosticType::UnexpectedTopLevelBox {
            box_type: FourCC(*b"free"),
            offset: 0,
        }));
        report.add(Diagnostic::new(DiagnosticType::BoxTooLargeToValidate {
            box_type: FourCC(*b"mdat"),
            size: 1000,
            limit: 100,
        }));

        assert_eq!(report.error_count(), 2);
        assert_eq!(report.warning_count(), 1);
        assert_eq!(report.info_count(), 1);
        assert_eq!(report.total_count(), 4);
        assert!(report.has_errors());
        assert!(!report.is_valid());
    }

    #[test]
    fn valid_report() {
        let report = ValidationReport::new();
        assert!(!report.has_errors());
        assert!(report.is_valid());
    }
}
