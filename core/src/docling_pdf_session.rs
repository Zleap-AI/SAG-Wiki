//! One local PDF owns one supervised converter, released at document end.
use super::*;
use crate::docling_worker::{ImageOutcome, SpawnSpec, Worker};
use std::{sync::RwLockReadGuard, time::Instant};

pub(crate) struct PdfSession<'a> {
    docling: &'a Docling,
    runtime: sag_protocol::runtime::RuntimeSettings,
    worker: Mutex<Option<Worker>>,
    // Keep the selected component installed until its last segment finishes.
    _usage: RwLockReadGuard<'a, ()>,
}

impl<'a> PdfSession<'a> {
    pub(super) fn new(
        docling: &'a Docling,
        runtime: &sag_protocol::runtime::RuntimeSettings,
    ) -> Self {
        Self {
            docling,
            runtime: runtime.clone(),
            worker: Mutex::new(None),
            _usage: docling.usage.read().unwrap(),
        }
    }

    pub(crate) fn convert(
        &self,
        source: &Path,
        output: &Path,
        cancel: &(impl Fn() -> bool + Sync),
    ) -> Result<()> {
        self.docling.convert_docling_runtime(
            source,
            output,
            &self.runtime,
            cancel,
            None,
            Some(self),
        )
    }

    pub(super) fn convert_raw(
        &self,
        source: &Path,
        output: &Path,
        deadline: Instant,
        cancel: &dyn Fn() -> bool,
    ) -> Result<ImageOutcome> {
        // External/legacy CLI installations retain their existing one-shot path.
        let Ok(component) =
            crate::docling_component::active_for(&self.docling.home, DoclingProfile::Documents)
        else {
            return Ok(ImageOutcome::Fallback);
        };
        let mut worker = self.worker.lock().unwrap();
        let exited = match worker.as_mut() {
            Some(worker) => !worker.is_running()?,
            None => false,
        };
        if exited {
            *worker = None;
        }
        if worker.is_none() {
            let mut runner = tempfile::NamedTempFile::new()?;
            runner.write_all(RUNNER.as_bytes())?;
            runner.flush()?;
            let runner = runner.into_temp_path();
            let mut command = self.docling.runtime_command(&component, &runner);
            command
                .arg("--pdf-worker")
                .arg("--artifacts-path")
                .arg(&component.models)
                .env(
                    "SAG_CONVERSION_SETTINGS",
                    serde_json::to_string(&self.runtime)?,
                );
            let (_, bytes, _) = crate::docling_budget::resource_summary(&self.runtime)
                .map_err(anyhow::Error::msg)?;
            let permit = crate::docling_budget::acquire_bytes_with_runtime(
                bytes,
                &self.runtime,
                cancel,
                deadline,
            )?;
            *worker = Some(Worker::spawn(
                SpawnSpec {
                    command,
                    runner,
                    runtime_root: component.root,
                    permit,
                    runtime: self.runtime.clone(),
                },
                deadline,
                cancel,
            )?);
        }
        let result = worker
            .as_mut()
            .unwrap()
            .convert(source, output, deadline, cancel);
        if result.is_err() || matches!(result, Ok(ImageOutcome::Fallback)) {
            // Release the reservation before the alternate backend may acquire it.
            *worker = None;
        }
        result
    }
}
