#![allow(clippy::type_complexity)]

use rustc_driver::Callbacks;
use rustc_interface::interface::Config;
use rustc_lint::LintStore;
use rustc_session::config::ErrorOutputType;
use rustc_session::EarlyErrorHandler;
use rustc_span::{ErrorGuaranteed, Symbol};

use std::sync::Arc;

struct Lints {
    callback: Arc<Box<dyn Fn(&mut LintStore) + Send + Sync + 'static>>,
    /// If one of these files is modified, the linter needs to be re-run.
    tracked_files: Arc<Vec<String>>,
}

impl Callbacks for Lints {
    fn config(&mut self, config: &mut Config) {
        // Should always be `None` but just in case...
        let previous = config.register_lints.take();

        let tracked_files = Arc::clone(&self.tracked_files);
        config.parse_sess_created = Some(Box::new(move |parse_sess| {
            // In here, we insert the files that, if modified, will tell cargo that the command
            // needs to be re-run.
            if tracked_files.is_empty() {
                return;
            }
            let file_depinfo = parse_sess.file_depinfo.get_mut();
            for tracked_file in tracked_files.iter() {
                file_depinfo.insert(Symbol::intern(tracked_file));
            }
        }));
        let callback = Arc::clone(&self.callback);
        config.register_lints = Some(Box::new(move |sess, lint_store| {
            if let Some(previous) = &previous {
                (previous)(sess, lint_store);
            }
            (*callback)(lint_store);
        }));
    }
}

/// If you want to create a linter, this the function you want to use.
///
/// * `args` is what is provided to the compiler.
/// * `tracked_files` is the files which will trigger a re-compilation if they are modified.
/// * `callback` is called when everything is setup. This is where you will register your lints
///   before they are run by rustc directly. It provides a mutable reference to the [`LintStore`]
///   type.
///
/// Take a look at the `examples/lint.rs` file if you want an example on how to create lints.
///
/// **VERY IMPORTANT TO NOTE**: if you want to run this code on a crate with dependencies, you'll
/// need to pass the according options so that `rustc` knows where to look for them. otherwise it
/// will simply fail to compile and the `callback` won't be called. A good example of the list
/// of the expected arguments can be seen when you run `cargo build -v`.
///
/// [`LintStore`]: https://doc.rust-lang.org/nightly/nightly-rustc/rustc_lint/struct.LintStore.html
pub fn with_lints<F: Fn(&mut LintStore) + Send + Sync + 'static>(
    args: &[String],
    tracked_files: Vec<String>,
    callback: F,
) -> Result<(), ErrorGuaranteed> {
    let handler = EarlyErrorHandler::new(ErrorOutputType::default());
    rustc_driver::init_rustc_env_logger(&handler);
    rustc_driver::catch_fatal_errors(move || {
        rustc_driver::RunCompiler::new(
            args,
            &mut Lints {
                callback: Arc::new(Box::new(callback)),
                tracked_files: Arc::new(tracked_files),
            },
        )
        .run()
        .map(|_| ())
    })?
}
