//! A safe wrapper around `llama_model_params`.

use crate::LlamaCppError;
use std::fmt::{Debug, Formatter};
use std::pin::Pin;

/// The maximum number of devices supported.
///
/// The real maximum number of devices is the lesser one of this value and the value returned by
/// `harbor_llama::max_devices()`.
pub const LLAMA_CPP_MAX_DEVICES: usize = 16;

/// A safe wrapper around `llama_model_params`.
#[allow(clippy::module_name_repetitions)]
pub struct LlamaModelParams {
    pub(crate) params: harbor_llama_sys::llama_model_params,
    /// Null-terminated device list `params.devices` points into (pinned so the
    /// pointer stays valid when the params move).
    devices: Pin<Box<[harbor_llama_sys::ggml_backend_dev_t; LLAMA_CPP_MAX_DEVICES]>>,
}

impl Debug for LlamaModelParams {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlamaModelParams")
            .field("n_gpu_layers", &self.params.n_gpu_layers)
            .field("main_gpu", &self.params.main_gpu)
            .field("devices", &self.devices)
            .finish()
    }
}

impl LlamaModelParams {
    /// Get the number of layers to offload to the GPU.
    #[must_use]
    pub fn n_gpu_layers(&self) -> i32 {
        self.params.n_gpu_layers
    }

    /// sets the number of gpu layers to offload to the GPU.
    /// ```
    /// # use harbor_llama::model::params::LlamaModelParams;
    /// let params = LlamaModelParams::default();
    /// let params = params.with_n_gpu_layers(1);
    /// assert_eq!(params.n_gpu_layers(), 1);
    /// ```
    #[must_use]
    pub fn with_n_gpu_layers(mut self, n_gpu_layers: u32) -> Self {
        // The only way this conversion can fail is if u32 overflows the i32 - in which case we set
        // to MAX
        let n_gpu_layers = i32::try_from(n_gpu_layers).unwrap_or(i32::MAX);
        self.params.n_gpu_layers = n_gpu_layers;
        self
    }

    /// sets `devices`
    ///
    /// The devices are specified as indices that correspond to the ggml backend device indices.
    /// An EMPTY slice yields an empty (null-terminated) device list, which is llama.cpp's
    /// CPU-only switch — zero GPU layers alone still lets ggml's scheduler offload ops.
    ///
    /// # Errors
    /// Returns `LlamaCppError::BackendDeviceNotFound` if any device index is invalid.
    pub fn with_devices(mut self, devices: &[usize]) -> Result<Self, LlamaCppError> {
        for dev in self.devices.iter_mut() {
            *dev = std::ptr::null_mut();
        }
        // Keep at least one slot for the null terminator.
        let max_devices = crate::max_devices().min(LLAMA_CPP_MAX_DEVICES - 1);
        if devices.len() > max_devices {
            return Err(LlamaCppError::MaxDevicesExceeded(max_devices));
        }
        for (i, &dev) in devices.iter().enumerate() {
            if dev >= unsafe { harbor_llama_sys::ggml_backend_dev_count() } {
                return Err(LlamaCppError::BackendDeviceNotFound(dev));
            }
            let backend_dev = unsafe { harbor_llama_sys::ggml_backend_dev_get(dev) };
            self.devices[i] = backend_dev;
        }
        self.params.devices = self.devices.as_mut_ptr();
        Ok(self)
    }
}

/// Default parameters for `LlamaModel` (`llama_model_default_params`).
/// ```
/// # use harbor_llama::model::params::LlamaModelParams;
/// let params = LlamaModelParams::default();
/// assert_eq!(params.n_gpu_layers(), -1, "n_gpu_layers should be -1");
/// ```
impl Default for LlamaModelParams {
    fn default() -> Self {
        let default_params = unsafe { harbor_llama_sys::llama_model_default_params() };
        LlamaModelParams {
            params: default_params,
            devices: Box::pin([std::ptr::null_mut(); LLAMA_CPP_MAX_DEVICES]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LlamaModelParams;

    #[test]
    fn empty_device_list_is_a_null_terminated_cpu_only_list() {
        let params = LlamaModelParams::default()
            .with_n_gpu_layers(0)
            .with_devices(&[])
            .expect("empty device list is valid");
        assert_eq!(params.n_gpu_layers(), 0);
        assert!(!params.params.devices.is_null());
        assert!(unsafe { *params.params.devices }.is_null());
    }

    #[test]
    fn unknown_device_index_is_rejected() {
        assert!(LlamaModelParams::default().with_devices(&[10_000]).is_err());
    }
}
