//! Minimal CUDA driver FFI helpers used by kernel timing and stream sync.

use rave_core::error::{EngineError, Result};
pub use rave_core::ffi_types::{CUDA_SUCCESS, CUevent, CUresult, CUstream};
use std::os::raw::c_uint;

unsafe extern "C" {
    pub fn cuEventCreate(phEvent: *mut CUevent, Flags: c_uint) -> CUresult;
    pub fn cuEventRecord(hEvent: CUevent, hStream: CUstream) -> CUresult;
    pub fn cuEventDestroy_v2(hEvent: CUevent) -> CUresult;
    pub fn cuStreamWaitEvent(hStream: CUstream, hEvent: CUevent, Flags: c_uint) -> CUresult;
}

#[inline]
pub fn check_cu(result: CUresult, context: &str) -> Result<()> {
    if result == CUDA_SUCCESS {
        Ok(())
    } else {
        Err(EngineError::Decode(format!(
            "{context} failed with CUDA error code {result}"
        )))
    }
}
