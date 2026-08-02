use mago_text_edit::{ApplyResult as MagoApplyResult, TextEdit as MagoEdit};
use ra_ap_text_edit::{
    TextEdit as RaEdit, TextEditBuilder as RaEditBuilder, TextRange as RaRange, TextSize as RaSize,
};
use typst_edit::Edit as TypstEdit;
use weavatrix_edit::{
    ApplyLimits, ByteEdit, PreparedEdits, Provenance, apply_byte_edits_with_limits,
    prepare_byte_edits_with_limits,
};

use crate::workload::{Workload, reference_apply};

#[derive(Clone, Debug)]
pub(crate) struct RaSpec {
    range: RaRange,
    replacement: String,
}

pub(crate) struct Adapters<'source> {
    pub(crate) workload: &'source Workload,
    pub(crate) limits: ApplyLimits,
    pub(crate) weavatrix_edits: Vec<ByteEdit>,
    pub(crate) weavatrix_prepared: PreparedEdits<'source>,
    pub(crate) mago_edits: Vec<MagoEdit>,
    pub(crate) mago_prepared: mago_text_edit::TextEditor<'source>,
    pub(crate) ra_specs: Vec<RaSpec>,
    pub(crate) ra_prepared: RaEdit,
    pub(crate) typst_edits: Vec<TypstEdit>,
    expected: String,
}

impl<'source> Adapters<'source> {
    pub(crate) fn new(workload: &'source Workload) -> Self {
        let expected = reference_apply(&workload.source, &workload.edits);
        let limits = ApplyLimits {
            max_source_bytes: workload.source.len(),
            max_edits: workload.edits.len(),
            max_output_bytes: expected.len(),
        };
        let weavatrix_edits = make_weavatrix_edits(workload);
        let weavatrix_prepared =
            prepare_byte_edits_with_limits(&workload.source, &weavatrix_edits, limits)
                .expect("generated Weavatrix edits must prepare");
        let mago_edits = make_mago_edits(workload);
        let mago_prepared = prepare_mago(&workload.source, mago_edits.clone());
        let ra_specs = make_ra_specs(workload);
        let ra_prepared = prepare_ra(ra_specs.clone());
        let typst_edits = make_typst_edits(workload);

        let adapters = Self {
            workload,
            limits,
            weavatrix_edits,
            weavatrix_prepared,
            mago_edits,
            mago_prepared,
            ra_specs,
            ra_prepared,
            typst_edits,
            expected,
        };
        adapters.verify_output_equivalence();
        assert!(
            !adapters.weavatrix_prepared.has_rendered_text(),
            "correctness verification must not warm the optional rendered cache"
        );
        adapters
    }

    fn verify_output_equivalence(&self) {
        let weavatrix =
            apply_byte_edits_with_limits(&self.workload.source, &self.weavatrix_edits, self.limits)
                .expect("Weavatrix one-shot path must apply")
                .text;
        assert_output("weavatrix-edit", &self.expected, &weavatrix);

        let mago = prepare_mago(&self.workload.source, self.mago_edits.clone()).finish();
        assert_output("mago-text-edit", self.expected.as_bytes(), &mago);

        let mut ra = self.workload.source.clone();
        prepare_ra(self.ra_specs.clone()).apply(&mut ra);
        assert_output("ra_ap_text_edit", &self.expected, &ra);

        let typst = typst_edit::apply(&self.workload.source, self.typst_edits.clone())
            .expect("generated Typst edits must apply");
        assert_output("typst-edit", &self.expected, &typst);

        let weavatrix_prepared = self.weavatrix_prepared.apply().text;
        assert_output(
            "weavatrix-edit prepared",
            &self.expected,
            &weavatrix_prepared,
        );

        let mut weavatrix_reused = String::with_capacity(self.expected.len());
        let summary = self.weavatrix_prepared.apply_into(&mut weavatrix_reused);
        assert_eq!(summary.bytes_before, self.workload.source.len());
        assert_eq!(summary.bytes_after, self.expected.len());
        assert_eq!(summary.edits_applied, self.workload.edits.len());
        assert_output(
            "weavatrix-edit caller buffer",
            &self.expected,
            &weavatrix_reused,
        );

        let mago_prepared = self.mago_prepared.clone().finish();
        assert_output(
            "mago-text-edit prepared",
            self.expected.as_bytes(),
            &mago_prepared,
        );
        let mut mago_reused = String::with_capacity(self.expected.len());
        mago_reused.push_str(
            std::str::from_utf8(mago_prepared.as_ref())
                .expect("benchmark replacements are valid UTF-8"),
        );
        assert_output("mago-text-edit caller buffer", &self.expected, &mago_reused);

        let mut ra_prepared = self.workload.source.clone();
        self.ra_prepared.apply(&mut ra_prepared);
        assert_output("ra_ap_text_edit prepared", &self.expected, &ra_prepared);

        let mut ra_reused = String::with_capacity(self.expected.len());
        ra_reused.push_str(&self.workload.source);
        self.ra_prepared.apply(&mut ra_reused);
        assert_output("ra_ap_text_edit caller buffer", &self.expected, &ra_reused);

        let mut weavatrix_bytes = Vec::with_capacity(self.expected.len());
        let byte_summary = self
            .weavatrix_prepared
            .apply_into_bytes(&mut weavatrix_bytes);
        assert_eq!(byte_summary.bytes_before, self.workload.source.len());
        assert_eq!(byte_summary.bytes_after, self.expected.len());
        assert_eq!(byte_summary.edits_applied, self.workload.edits.len());
        assert_output(
            "weavatrix-edit caller Vec",
            self.expected.as_bytes(),
            &weavatrix_bytes,
        );

        let mut mago_bytes = Vec::with_capacity(self.expected.len());
        mago_bytes.extend_from_slice(mago_prepared.as_ref());
        assert_output(
            "mago-text-edit caller Vec",
            self.expected.as_bytes(),
            &mago_bytes,
        );

        let mut ra_bytes = Vec::with_capacity(self.expected.len());
        ra_bytes.extend_from_slice(ra_reused.as_bytes());
        assert_output(
            "ra_ap_text_edit caller Vec",
            self.expected.as_bytes(),
            &ra_bytes,
        );

        let chunked = self.weavatrix_prepared.chunks().collect::<String>();
        assert_output("weavatrix-edit chunks", &self.expected, chunked);

        let mut written = Vec::with_capacity(self.expected.len());
        let summary = self
            .weavatrix_prepared
            .write_to(&mut written)
            .expect("Vec writes cannot fail");
        assert_eq!(summary.bytes_written, self.expected.len());
        assert_eq!(summary.edits_applied, self.workload.edits.len());
        assert_output("weavatrix-edit write_to", self.expected.as_bytes(), written);
    }
}

fn make_weavatrix_edits(workload: &Workload) -> Vec<ByteEdit> {
    workload
        .edits
        .iter()
        .map(|edit| {
            ByteEdit::replace(
                edit.start..edit.end,
                edit.before.clone(),
                edit.after.clone(),
                Provenance::EXACT_LSP,
            )
        })
        .collect()
}

fn make_mago_edits(workload: &Workload) -> Vec<MagoEdit> {
    workload
        .edits
        .iter()
        .map(|edit| {
            MagoEdit::replace(
                u32::try_from(edit.start).expect("benchmark offset fits u32")
                    ..u32::try_from(edit.end).expect("benchmark offset fits u32"),
                edit.after.as_bytes().to_vec(),
            )
        })
        .collect()
}

fn make_ra_specs(workload: &Workload) -> Vec<RaSpec> {
    workload
        .edits
        .iter()
        .map(|edit| RaSpec {
            range: RaRange::new(to_ra_size(edit.start), to_ra_size(edit.end)),
            replacement: edit.after.clone(),
        })
        .collect()
}

fn make_typst_edits(workload: &Workload) -> Vec<TypstEdit> {
    workload
        .edits
        .iter()
        .map(|edit| TypstEdit::new(edit.start..edit.end, edit.after.clone()))
        .collect()
}

pub(crate) fn prepare_mago(source: &str, edits: Vec<MagoEdit>) -> mago_text_edit::TextEditor<'_> {
    let mut editor = mago_text_edit::TextEditor::new(source.as_bytes());
    let result = editor.apply_batch(edits, None::<fn(&[u8]) -> bool>);
    assert_eq!(
        result,
        MagoApplyResult::Applied,
        "generated Mago edits must prepare"
    );
    editor
}

pub(crate) fn prepare_ra(specs: Vec<RaSpec>) -> RaEdit {
    let mut builder = RaEditBuilder::default();
    for spec in specs {
        builder.replace(spec.range, spec.replacement);
    }
    builder.finish()
}

fn to_ra_size(offset: usize) -> RaSize {
    RaSize::from(u32::try_from(offset).expect("benchmark offset fits rust-analyzer u32"))
}

fn assert_output(label: &str, expected: impl AsRef<[u8]>, actual: impl AsRef<[u8]>) {
    let expected = expected.as_ref();
    let actual = actual.as_ref();
    assert_eq!(
        actual.len(),
        expected.len(),
        "{label} output length differs"
    );
    assert_eq!(actual, expected, "{label} output bytes differ");
}
