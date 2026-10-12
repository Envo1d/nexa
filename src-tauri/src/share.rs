//! Windows share sheet; registration stays alive on the window's UI thread.
#[cfg(windows)]
pub fn show(app: tauri::AppHandle, path: &std::path::Path) -> Result<(), String> {
    use tauri::Manager;
    use windows::{
        Storage::StorageFile,
        core::{AgileReference, HSTRING},
    };

    let _apartment = crate::windows_com::Apartment::initialize()
        .map_err(|_| "Cannot initialize Windows sharing")?;

    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(
        path.to_str().ok_or("Invalid file path")?,
    ))
    .and_then(|operation| operation.join())
    .map_err(|_| "Cannot prepare the file for sharing")?;

    let file = AgileReference::new(&file).map_err(|_| "Cannot marshal shared file")?;

    let window = app
        .get_webview_window("main")
        .ok_or("Main window unavailable")?;

    let hwnd = window.hwnd().map_err(|_| "Window handle unavailable")?.0 as usize;

    let (tx, rx) = std::sync::mpsc::sync_channel(1);
    app.run_on_main_thread(move || {
        let result =
            show_on_ui(hwnd, file).map_err(|_| "Windows sharing is unavailable".to_string());

        let _ = tx.send(result);
    })
    .map_err(|_| "Cannot schedule Windows sharing")?;
    rx.recv().map_err(|_| "Sharing UI worker stopped")?
}

#[cfg(windows)]
fn show_on_ui(
    hwnd: usize,
    file: windows::core::AgileReference<windows::Storage::StorageFile>,
) -> windows::core::Result<()> {
    use windows::{
        ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager},
        Foundation::TypedEventHandler,
        Win32::{Foundation::HWND, UI::Shell::IDataTransferManagerInterop},
        core::{HSTRING, Interface},
    };
    thread_local! {
        static REGISTRATION: std::cell::RefCell<Option<(DataTransferManager, i64)>> = const { std::cell::RefCell::new(None) };
    }

    let interop: IDataTransferManagerInterop =
        windows::core::factory::<DataTransferManager, IDataTransferManagerInterop>()?;

    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: handle belongs to the live main window and calls run on its UI thread.
    let manager: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
    REGISTRATION.with(|slot| {
        if let Some((previous, token)) = slot.borrow_mut().take() {
            let _ = previous.RemoveDataRequested(token);
        }
    });

    let token = manager.DataRequested(&TypedEventHandler::<
        DataTransferManager,
        DataRequestedEventArgs,
    >::new(move |_, args| {
        let result = (|| {
            let data = args
                .as_ref()
                .ok_or_else(|| {
                    windows::core::Error::from_hresult(windows::core::HRESULT(0x80004003u32 as i32))
                })?
                .Request()?
                .Data()?;
            data.Properties()?.SetTitle(&HSTRING::from("Nexa media"))?;

            let items: windows_collections::IIterable<windows::Storage::IStorageItem> =
                vec![Some(file.resolve()?.cast()?)].into();
            data.SetStorageItemsReadOnly(&items)
        })();

        if result.is_err() {
            tracing::error!("Windows share data request failed");
        }
        result
    }))?;
    // SAFETY: same live HWND and UI-thread requirement as GetForWindow.
    if let Err(error) = unsafe { interop.ShowShareUIForWindow(hwnd) } {
        let _ = manager.RemoveDataRequested(token);

        return Err(error);
    }
    REGISTRATION.with(|slot| *slot.borrow_mut() = Some((manager, token)));

    tracing::info!("Windows share sheet opened");
    Ok(())
}

#[cfg(not(windows))]
pub fn show(_app: tauri::AppHandle, _path: &std::path::Path) -> Result<(), String> {
    Err("System sharing is currently supported on Windows only".into())
}
