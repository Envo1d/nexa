//! Modal backdrop from this process's own window. Frames remain in memory;
//! never enumerate/capture displays, other applications or persistent files.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::RenderImage;
use windows::{
    Graphics::{
        Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession},
        DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat},
    },
    Win32::{
        Foundation::{HMODULE, HWND},
        Graphics::{
            Direct3D::D3D_DRIVER_TYPE_HARDWARE,
            Direct3D11::{
                D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAP_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
                D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Texture2D,
            },
            Dxgi::IDXGIDevice,
        },
        System::WinRT::{
            Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess},
            Graphics::Capture::IGraphicsCaptureItemInterop,
            RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize,
        },
        UI::{
            Input::KeyboardAndMouse::GetActiveWindow,
            WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
    },
    core::{Interface, factory},
};

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: created only after successful initialization on this worker.
        unsafe { RoUninitialize() };
    }
}

struct Capture {
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}

pub(super) fn capture(scale: f32) -> Option<Arc<RenderImage>> {
    // SAFETY: query the active HWND on the calling UI thread. Never discover a
    // different process's window or fall back to a display capture source.
    let window = unsafe { GetActiveWindow() };
    let mut process = 0;
    unsafe {
        GetWindowThreadProcessId(window, Some(&mut process));
    }
    let owned_foreground = !window.is_invalid()
        && process == std::process::id()
        && unsafe { GetForegroundWindow() } == window;
    if !owned_foreground {
        tracing::warn!(
            operation = "modal_backdrop",
            "Own foreground window unavailable; using tint fallback"
        );
        return None;
    }
    let hwnd = window.0 as usize;
    let result = std::thread::spawn(move || capture_pixels(hwnd)).join();
    let pixels = match result {
        Ok(Ok(pixels)) => pixels,
        Ok(Err(error)) => {
            tracing::warn!(
                operation = "modal_backdrop",
                code = error.code().0,
                "Window capture failed; using tint fallback"
            );
            return None;
        }
        Err(_) => {
            tracing::warn!(
                operation = "modal_backdrop",
                "Window capture worker failed; using tint fallback"
            );
            return None;
        }
    };
    let mut pixels = image::imageops::fast_blur(&pixels, (5. * scale).max(0.1));
    for pixel in pixels.pixels_mut() {
        // CSS --backdrop: rgb(5 4 12 / 67%). GPU frames/RenderImage use BGRA.
        for (channel, tint) in pixel.0[..3].iter_mut().zip([12., 4., 5.]) {
            *channel = (f32::from(*channel) * 0.33 + tint * 0.67).round() as u8;
        }
        pixel.0[3] = 255;
    }
    tracing::debug!(
        operation = "modal_backdrop",
        "Prepared own-window modal backdrop"
    );

    Some(Arc::new(RenderImage::new(vec![image::Frame::new(pixels)])))
}

fn capture_pixels(hwnd: usize) -> windows::core::Result<image::RgbaImage> {
    // A dedicated MTA worker lets the free-threaded capture pool deliver its
    // initial compositor frame while the owning UI operation holds its draft.
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }?;
    let _apartment = Apartment;
    let window = HWND(hwnd as *mut std::ffi::c_void);
    let mut process = 0;
    unsafe {
        GetWindowThreadProcessId(window, Some(&mut process));
    }
    if process != std::process::id() {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x80070005_u32 as i32,
        )));
    }
    let interop: IGraphicsCaptureItemInterop = factory::<GraphicsCaptureItem, _>()?;
    // SAFETY: HWND was verified as owned immediately before creating this item.
    let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(window) }?;
    let size = item.Size()?;
    if size.Width <= 0
        || size.Height <= 0
        || i64::from(size.Width) * i64::from(size.Height) > 16_777_216
    {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x80070057_u32 as i32,
        )));
    }
    let mut device = None;
    let mut context = None;
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }?;
    let device = device.ok_or_else(windows::core::Error::from_thread)?;
    let context = context.ok_or_else(windows::core::Error::from_thread)?;
    let dxgi: IDXGIDevice = device.cast()?;
    let runtime_device: IDirect3DDevice =
        unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }?.cast()?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &runtime_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        2,
        size,
    )?;
    let session = pool.CreateCaptureSession(&item)?;
    let capture = Capture { pool, session };
    capture.session.SetIsCursorCaptureEnabled(false)?;
    capture.session.StartCapture()?;
    let deadline = Instant::now() + Duration::from_millis(750);
    let frame = loop {
        if let Ok(frame) = capture.pool.TryGetNextFrame() {
            break frame;
        }
        if Instant::now() >= deadline {
            return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                0x800705B4_u32 as i32,
            )));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
    let texture: ID3D11Texture2D = unsafe { access.GetInterface() }?;
    let mut description = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&mut description) };
    if size.Width as u32 > description.Width || size.Height as u32 > description.Height {
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x80070057_u32 as i32,
        )));
    }
    description.Usage = D3D11_USAGE_STAGING;
    description.BindFlags = 0;
    description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    description.MiscFlags = 0;
    let mut staging = None;
    unsafe { device.CreateTexture2D(&description, None, Some(&mut staging)) }?;
    let staging = staging.ok_or_else(windows::core::Error::from_thread)?;
    unsafe { context.CopyResource(&staging, &texture) };
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe { context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }?;
    let width = size.Width as u32;
    let height = size.Height as u32;
    if mapped.pData.is_null() || (mapped.RowPitch as usize) < width as usize * 4 {
        unsafe { context.Unmap(&staging, 0) };
        return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
            0x80070057_u32 as i32,
        )));
    }
    let mut bytes = vec![0_u8; width as usize * height as usize * 4];
    for (row, target) in bytes.chunks_exact_mut(width as usize * 4).enumerate() {
        // SAFETY: successful Map owns RowPitch bytes for each texture row.
        // The capture content dimensions are bounded by this texture's extent.
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped
                    .pData
                    .cast::<u8>()
                    .add(row * mapped.RowPitch as usize),
                target.as_mut_ptr(),
                target.len(),
            );
        }
    }
    unsafe { context.Unmap(&staging, 0) };
    let _ = frame.Close();

    image::RgbaImage::from_raw(width, height, bytes).ok_or_else(windows::core::Error::from_thread)
}
