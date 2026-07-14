//! Audited Windows-only boundary for HWND-bound Windows Hello consent.
//!
//! The generated COM call is inherently unsafe. It is isolated here so the
//! Core, Runtime and Tauri business host remain `unsafe_code = "forbid"`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentResult {
    Verified,
    Canceled,
    NotConfigured,
    DisabledByPolicy,
    DeviceUnavailable,
    RetriesExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentError;

#[cfg(windows)]
pub fn verify_for_window(hwnd: isize, message: &str) -> Result<ConsentResult, ConsentError> {
    use windows::Security::Credentials::UI::{UserConsentVerificationResult, UserConsentVerifier};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    use windows::core::{HSTRING, factory};
    use windows_future::IAsyncOperation;

    let interop =
        factory::<UserConsentVerifier, IUserConsentVerifierInterop>().map_err(|_| ConsentError)?;
    let operation: IAsyncOperation<UserConsentVerificationResult> = unsafe {
        // SAFETY: `hwnd` originates from Tauri's live main WebviewWindow and is
        // only used for the duration of this awaited OS-owned consent dialog.
        interop.RequestVerificationForWindowAsync(
            HWND(hwnd as *mut core::ffi::c_void),
            &HSTRING::from(message),
        )
    }
    .map_err(|_| ConsentError)?;
    let result = operation.get().map_err(|_| ConsentError)?;
    Ok(if result == UserConsentVerificationResult::Verified {
        ConsentResult::Verified
    } else if result == UserConsentVerificationResult::Canceled {
        ConsentResult::Canceled
    } else if result == UserConsentVerificationResult::NotConfiguredForUser {
        ConsentResult::NotConfigured
    } else if result == UserConsentVerificationResult::DisabledByPolicy {
        ConsentResult::DisabledByPolicy
    } else if result == UserConsentVerificationResult::RetriesExhausted {
        ConsentResult::RetriesExhausted
    } else {
        ConsentResult::DeviceUnavailable
    })
}

#[cfg(not(windows))]
pub fn verify_for_window(_hwnd: isize, _message: &str) -> Result<ConsentResult, ConsentError> {
    Err(ConsentError)
}
