use alloc::{borrow::ToOwned, sync::Arc};

use miden_lib::transaction::TransactionKernel;
use miden_objects::assembly::{
    SourceManager,
    debuginfo::{SourceLanguage, Uri},
};
use vm_processor::{
    AdviceInputs, DefaultHost, ExecutionError, Process, Program, StackInputs, SyncHost,
};

// MOCK CODE EXECUTOR
// ================================================================================================

/// Helper for executing arbitrary code within arbitrary hosts.
pub struct CodeExecutor<H> {
    host: H,
    stack_inputs: Option<StackInputs>,
    advice_inputs: AdviceInputs,
}

impl<H: SyncHost> CodeExecutor<H> {
    // CONSTRUCTOR
    // --------------------------------------------------------------------------------------------
    pub(crate) fn new(host: H) -> Self {
        Self {
            host,
            stack_inputs: None,
            advice_inputs: AdviceInputs::default(),
        }
    }

    pub fn extend_advice_inputs(mut self, advice_inputs: AdviceInputs) -> Self {
        self.advice_inputs.extend(advice_inputs);
        self
    }

    pub fn stack_inputs(mut self, stack_inputs: StackInputs) -> Self {
        self.stack_inputs = Some(stack_inputs);
        self
    }

    /// Compiles and runs the desired code in the host and returns the [`Process`] state.
    ///
    /// To improve the error message quality, convert the returned [`ExecutionError`] into a
    /// [`Report`](miden_objects::assembly::diagnostics::Report).
    pub fn run(self, code: &str) -> Result<Process, ExecutionError> {
        let assembler = TransactionKernel::testing_assembler().with_debug_mode(true);
        // TODO: Proper fix.
        let source_manager =
            alloc::sync::Arc::new(miden_objects::assembly::DefaultSourceManager::default())
                as alloc::sync::Arc<dyn miden_objects::assembly::SourceManager>;

        // Virtual file name should be unique.
        let virtual_source_file =
            source_manager.load(SourceLanguage::Masm, Uri::new("_user_code"), code.to_owned());
        let program = assembler.assemble_program(virtual_source_file).unwrap();

        self.execute_program(program, source_manager)
    }

    /// Executes the provided [`Program`] and returns the [`Process`] state.
    ///
    /// To improve the error message quality, convert the returned [`ExecutionError`] into a
    /// [`Report`](miden_objects::assembly::diagnostics::Report).
    pub fn execute_program(
        mut self,
        program: Program,
        source_manager: Arc<dyn SourceManager>,
    ) -> Result<Process, ExecutionError> {
        let mut process = Process::new_debug(
            program.kernel().clone(),
            self.stack_inputs.unwrap_or_default(),
            self.advice_inputs,
        )
        .with_source_manager(source_manager);
        process.execute(&program, &mut self.host)?;

        Ok(process)
    }
}

impl CodeExecutor<DefaultHost> {
    pub fn with_default_host() -> Self {
        let mut host = DefaultHost::default();

        let test_lib = TransactionKernel::kernel_as_library();
        host.load_library(test_lib.mast_forest()).unwrap();

        CodeExecutor::new(host)
    }
}
