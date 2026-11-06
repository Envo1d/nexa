use std::{path::PathBuf, time::Duration};

use gpui::{Context, FocusHandle, PathPromptOptions, Window};

use super::{Gallery, backup};

mod presentation;
mod updater;

pub(in crate::ui) struct Workflow {
    pub(super) preview: Option<backup::Preview>,
    pub confirmed: bool,
    pub(super) update: Update,
    pub modal: bool,
    pub focus: FocusHandle,
    pub cancel: FocusHandle,
    pub install: FocusHandle,
    pub return_focus: Option<FocusHandle>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Update {
    Unchecked,
    Checking,
    Available,
    Complete,
}

impl Workflow {
    pub fn new(cx: &mut Context<Gallery>) -> Self {
        Self {
            preview: None,
            confirmed: false,
            update: Update::Unchecked,
            modal: false,
            focus: cx.focus_handle(),
            cancel: cx.focus_handle().tab_stop(true),
            install: cx.focus_handle().tab_stop(true),
            return_focus: None,
        }
    }
}

impl Gallery {
    fn settings_snapshot(&self) -> backup::Snapshot {
        backup::Snapshot::new(
            self.settings.paths.clone(),
            self.entries.clone(),
            self.inactive_workspace.entries.clone(),
        )
    }

    pub(in crate::ui) fn settings_browse(
        &mut self,
        section: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_open {
            return;
        }
        self.dialog_open = true;
        let prompt = crate::dialogs::open(
            window,
            cx,
            PathPromptOptions {
                files: false,
                directories: true,
                multiple: false,
                prompt: Some(crate::i18n::text("Выбрать папку").into()),
            },
            "settings_directory",
        );
        cx.spawn(async move |this, cx| {
            let result = prompt.await;
            let _ = this.update(cx, |this, cx| {
                this.dialog_open = false;
                match result {
                    Ok(Some(paths)) => {
                        if let Some(path) = paths.first() {
                            this.settings.paths[section] =
                                Some(path.to_string_lossy().into_owned());
                            this.settings.saved = false;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => this.settings_result(Err(error), "settings_directory"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::ui) fn settings_export(
        &mut self,
        portable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_open {
            return;
        }
        self.dialog_open = true;
        self.settings.message = None;
        let snapshot = self.settings_snapshot();
        let directory = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_default();
        let name = format!(
            "nexa-gpui-{}-{}.json",
            if portable { "metadata" } else { "session" },
            chrono::Utc::now().format("%Y-%m-%d-%H-%M-%S")
        );
        let prompt = crate::dialogs::save(window, cx, directory, name);
        tracing::info!(
            operation = "session_backup_export",
            portable,
            "Export destination requested"
        );
        cx.spawn(async move |this, cx| {
            let result = match prompt.await {
                Ok(Some(path)) => {
                    cx.background_executor()
                        .spawn(async move {
                            backup::write(&path, &snapshot, portable).map(|_| {
                                crate::i18n::text("Копия сеанса GPUI сохранена. Производственный каталог не затронут.")
                                    .to_owned()
                            })
                        })
                        .await
                }
                Ok(None) => Ok(String::new()),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                this.dialog_open = false;
                this.settings_result(result, "session_backup_export");
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::ui) fn settings_choose_backup(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_open {
            return;
        }
        self.dialog_open = true;
        self.settings.workflow.preview = None;
        self.settings.workflow.confirmed = false;
        self.settings.message = None;
        let prompt = crate::dialogs::open(
            window,
            cx,
            PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(crate::i18n::text("Выбрать JSON-копию сеанса GPUI").into()),
            },
            "session_backup_inspect",
        );
        cx.spawn(async move |this, cx| {
            let result = match prompt.await {
                Ok(Some(paths)) => match paths.into_iter().next() {
                    Some(path) => {
                        cx.background_executor()
                            .spawn(async move { backup::inspect(path).map(Some) })
                            .await
                    }
                    None => Ok(None),
                },
                Ok(None) => Ok(None),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                this.dialog_open = false;
                match result {
                    Ok(preview) => {
                        this.settings.workflow.preview = preview;
                        tracing::info!(
                            operation = "session_backup_inspect",
                            selected = this.settings.workflow.preview.is_some(),
                            "Backup inspection completed"
                        );
                    }
                    Err(error) => this.settings_result(Err(error), "session_backup_inspect"),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::ui) fn settings_restore(&mut self, cx: &mut Context<Self>) {
        if self.dialog_open || !self.settings.workflow.confirmed {
            return;
        }
        let Some(preview) = self.settings.workflow.preview.take() else {
            return;
        };
        self.dialog_open = true;
        let current = self.settings_snapshot();
        tracing::info!(
            operation = "session_backup_restore",
            "Session restore started"
        );
        cx.spawn(async move |this, cx| {
            let (preview, result) = cx
                .background_executor()
                .spawn(async move {
                    let result = backup::prepare_restore(&preview, &current);
                    (preview, result)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.dialog_open = false;
                match result {
                    Ok(path) => {
                        this.release_previews(cx);
                        this.entries = preview.snapshot.library;
                        this.inactive_workspace.entries = preview.snapshot.incoming;
                        this.inactive_workspace.previews.clear();
                        this.inactive_workspace.selected = None;
                        this.inactive_workspace.selection.clear();
                        this.selected = None;
                        this.stage_selection.clear();
                        this.tag_counts = crate::catalog::tag_counts(&this.entries);
                        this.refilter(cx);
                        this.settings.workflow.confirmed = false;
                        this.settings_result(
                            Ok(if crate::i18n::english() {
                                format!(
                                    "Session catalog restored. Safety backup: {}",
                                    path.display()
                                )
                            } else {
                                format!(
                                    "Каталог сеанса восстановлен. Защитная копия: {}",
                                    path.display()
                                )
                            }),
                            "session_backup_restore",
                        );
                    }
                    Err(error) => {
                        this.settings.workflow.preview = Some(preview);
                        this.settings_result(Err(error), "session_backup_restore");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn settings_result(&mut self, result: Result<String, String>, operation: &'static str) {
        self.settings.message = Some(match result {
            Ok(message) => {
                tracing::info!(operation, "Settings operation completed");
                message
            }
            Err(error) => {
                tracing::error!(operation, reason = %error, "Settings operation failed");
                if crate::i18n::english() {
                    format!("Error: {error}")
                } else {
                    format!("Ошибка: {error}")
                }
            }
        });
    }

    pub(in crate::ui) fn settings_check_update(&mut self, cx: &mut Context<Self>) {
        if self.dialog_open || self.settings.workflow.update == Update::Checking {
            return;
        }
        self.settings.workflow.update = Update::Checking;
        tracing::info!(
            operation = "update_check",
            mode = "simulation",
            "Update check preview started"
        );
        cx.spawn(async |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(800))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.settings.workflow.update = Update::Available;
                tracing::info!(
                    operation = "update_check",
                    mode = "simulation",
                    "Update check preview completed"
                );
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::ui) fn settings_confirm_update(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_open || self.settings.workflow.update != Update::Available {
            return;
        }
        let workflow = &mut self.settings.workflow;
        workflow.return_focus = window.focused(cx);
        workflow.modal = true;
        workflow.cancel.focus(window);
        cx.notify();
    }

    pub(in crate::ui) fn settings_close_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workflow = &mut self.settings.workflow;
        workflow.modal = false;
        workflow
            .return_focus
            .as_ref()
            .unwrap_or(&self.focus)
            .focus(window);
        cx.notify();
    }

    pub(in crate::ui) fn settings_modal_tab(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workflow = &self.settings.workflow;
        if workflow.cancel.is_focused(window) {
            workflow.install.focus(window);
        } else if workflow.install.is_focused(window) || !backwards {
            workflow.cancel.focus(window);
        } else {
            workflow.install.focus(window);
        }
        cx.notify();
    }

    fn settings_install_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings_close_modal(window, cx);
        match updater::open(cx) {
            Ok(()) => self.settings.workflow.update = Update::Complete,
            Err(error) => self.settings_result(Err(error), "update_preview"),
        }
        cx.notify();
    }
}
