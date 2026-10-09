//! Thread-bound COM lifetime shared by the unused native media providers.
use std::{marker::PhantomData, rc::Rc};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

// !Send/!Sync: initialization and teardown must occur on the same thread.
pub(crate) struct Apartment(PhantomData<Rc<()>>);

impl Apartment {
    pub(crate) fn initialize() -> windows::core::Result<Self> {
        // SAFETY: reserved argument is null. Both S_OK and S_FALSE require teardown.
        // Failed initialization (including RPC_E_CHANGED_MODE) creates no guard.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok()?;
        Ok(Self(PhantomData))
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: successful initialization on this thread; guard cannot move threads.
        unsafe { CoUninitialize() };
    }
}
