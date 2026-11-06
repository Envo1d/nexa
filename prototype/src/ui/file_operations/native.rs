use std::path::Path;

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::{
    Storage::StorageFile,
    core::{AgileReference, HSTRING},
};

pub(super) fn owner(window: &gpui::Window) -> Result<usize, String> {
    match HasWindowHandle::window_handle(window)
        .map_err(|_| "Window handle unavailable")?
        .as_raw()
    {
        RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get() as usize),
        _ => Err("Windows sharing requires a Windows window".into()),
    }
}

pub(super) fn prepare(path: &Path) -> Result<AgileReference<StorageFile>, String> {
    let _apartment = crate::windows_com::Apartment::initialize()
        .map_err(|_| "Cannot initialize Windows sharing")?;
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(
        path.to_str().ok_or("Invalid file path")?,
    ))
    .and_then(|operation| operation.join())
    .map_err(|_| "Cannot prepare the file for sharing")?;

    AgileReference::new(&file).map_err(|_| "Cannot marshal shared file".into())
}

pub(super) fn open(path: &Path, reveal: bool) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        Win32::UI::{
            Shell::{SHOpenFolderAndSelectItems, SHParseDisplayName, ShellExecuteW},
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
        core::PCWSTR,
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    if wide[..wide.len() - 1].contains(&0) {
        return Err("Invalid file path".into());
    }
    let _apartment = crate::windows_com::Apartment::initialize()
        .map_err(|_| "Cannot initialize Windows Shell")?;

    // SAFETY: The NUL-terminated path and PIDL remain valid through each Shell call.
    unsafe {
        if reveal {
            let mut item = std::ptr::null_mut();
            SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut item, 0, None)
                .map_err(|_| "Cannot locate the file in Explorer")?;
            let result = SHOpenFolderAndSelectItems(item, None, 0);
            windows::Win32::System::Com::CoTaskMemFree(Some(item.cast()));
            result.map_err(|_| "Cannot reveal the file".into())
        } else {
            let result = ShellExecuteW(
                None,
                windows::core::w!("open"),
                PCWSTR(wide.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            );
            if result.0 as isize <= 32 {
                Err("Windows could not open the file".into())
            } else {
                Ok(())
            }
        }
    }
}

pub(super) fn show(
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
