use super::*;
use crate::domain::RunExportOptions;

impl PactrunApplication {
    pub(crate) fn create_run_export_stage(&self) -> Result<StagedFile, ApplicationError> {
        Ok(self.staging()?.create_managed_output_stage()?)
    }
    pub(crate) fn write_run_export(
        &self,
        run: RunId,
        options: RunExportOptions,
        sink: &mut impl crate::persistence::RunExportSink,
    ) -> Result<(), ApplicationError> {
        options
            .validate()
            .map_err(|message| ApplicationError::InvalidRequest(message.into()))?;
        Ok(self.persistence.write_run_export(run, options, sink)?)
    }
}
