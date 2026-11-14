//! Test-only scoped faults at the existing production publication owner.
//! The normal build has no hook or alternate saver.
use super::*;

#[derive(Debug, Clone, Copy)]
pub(crate) enum Fault {
    Io(SaveFailurePoint, ErrorKind),
    Publication(SaveFailurePoint, PublicationEffect),
    Future { stage: SaveStage, backup: bool },
    PublishedThenError,
    DirectoryParent,
    UnreadableInspection { backup: bool },
}
thread_local! {
    static NEXT:std::cell::Cell<Option<Fault>>=const {std::cell::Cell::new(None)};
    static ERRORS:std::cell::RefCell<Vec<SaveError>>=const {std::cell::RefCell::new(Vec::new())};
}
pub(crate) fn scoped<T>(fault: Fault, body: impl FnOnce() -> T) -> (T, Vec<SaveError>) {
    struct Restore(Option<Fault>, Vec<SaveError>);
    impl Drop for Restore {
        fn drop(&mut self) {
            NEXT.set(self.0);
            ERRORS.set(std::mem::take(&mut self.1));
        }
    }
    let previous = NEXT.replace(Some(fault));
    let _restore = Restore(previous, ERRORS.replace(Vec::new()));
    let value = body();
    (value, ERRORS.take())
}
pub(super) fn take() -> Option<Fault> {
    NEXT.take()
}
impl Fault {
    pub(super) fn save(
        self,
        state: &CandidateState,
        path: &Path,
    ) -> Result<SaveOutcome, SaveError> {
        let result = match self {
            Self::Io(point, kind) => {
                state.save_impl(path, Some(SaveFault { point, kind }), None, |_| {})
            }
            Self::Publication(point, effect) => {
                state.save_impl(path, None, Some(PublicationFault { point, effect }), |_| {})
            }
            Self::Future { stage, backup } => state.save_impl(path, None, None, |checkpoint| {
                if checkpoint == stage {
                    fs::write(
                        if backup {
                            path.with_extension("toml.bak")
                        } else {
                            path.to_path_buf()
                        },
                        b"# owned injected late future file\nversion=999\n",
                    )
                    .unwrap();
                }
            }),
            Self::PublishedThenError => state.save(path).and_then(|_| {
                Err(observed_save_error(
                    SaveStage::PublishPrimary,
                    path,
                    &path.with_extension("toml.bak"),
                    "injected post-publication observation failure; primary bytes were published",
                ))
            }),
            // The caller has created an owned regular file at the proposed
            // parent path, so the production create_dir_all itself must fail.
            Self::DirectoryParent => state.save(path),
            Self::UnreadableInspection { backup } => {
                state.save_impl(path, None, None, |checkpoint| {
                    if checkpoint == SaveStage::InspectExisting {
                        let target = if backup {
                            path.with_extension("toml.bak")
                        } else {
                            path.to_path_buf()
                        };
                        let recovery = target.with_extension("inspection-original");
                        assert!(!recovery.exists());
                        fs::rename(&target, &recovery).unwrap();
                        fs::create_dir(&target).unwrap();
                    }
                })
            }
        };
        if let Err(error) = &result {
            ERRORS.with_borrow_mut(|errors| errors.push(error.clone()));
        }
        result
    }
}
