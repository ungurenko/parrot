/// Hardware capability probes used to pick the fastest safe processing mode.
///
/// Parrot runs on Apple Silicon Macs with anywhere from 8 GB of unified
/// memory upward. The fp16 MLX acceleration path keeps ~1.5 GB resident
/// (weights + activations), which is comfortable on 16 GB machines but
/// crowds out the system on 8 GB base models. Those stay on the int8
/// ONNX CPU engine, which peaks at roughly a third of that footprint.
use std::sync::atomic::{AtomicU8, Ordering};

/// 14 GiB separates 8 GB machines from 16 GB+ machines with margin for
/// reported "usable" RAM quirks.
const MLX_MIN_RAM_BYTES: u64 = 14 * 1024 * 1024 * 1024;

// 0 = unknown, 1 = disallowed, 2 = allowed. Probed once; RAM never changes.
static MLX_ALLOWED: AtomicU8 = AtomicU8::new(0);

pub(crate) fn total_ram_bytes() -> Option<u64> {
    let mut value: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let name = b"hw.memsize\0";
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr() as *const libc::c_char,
            &mut value as *mut u64 as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        None
    } else {
        Some(value)
    }
}

/// Pure decision so tests can cover the threshold without sysctl.
fn ram_allows_mlx(ram: Option<u64>) -> bool {
    match ram {
        Some(bytes) => bytes >= MLX_MIN_RAM_BYTES,
        // Unknown hardware: keep the acceleration enabled rather than
        // silently downgrading machines we simply failed to probe.
        None => true,
    }
}

/// Whether this machine has enough unified memory to prefer the fp16 MLX
/// Parakeet runtime over the int8 ONNX one.
pub(crate) fn mlx_acceleration_allowed() -> bool {
    match MLX_ALLOWED.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let allowed = ram_allows_mlx(total_ram_bytes());
            MLX_ALLOWED.store(if allowed { 2 } else { 1 }, Ordering::Relaxed);
            allowed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_ram_returns_sane_value() {
        let ram = total_ram_bytes();
        if let Some(bytes) = ram {
            assert!(bytes >= 4 * 1024 * 1024 * 1024, "implausible RAM: {bytes}");
        }
    }

    #[test]
    fn eight_gb_machines_stay_on_onnx() {
        assert!(!ram_allows_mlx(Some(8 * 1024 * 1024 * 1024)));
    }

    #[test]
    fn sixteen_gb_and_up_get_mlx() {
        assert!(ram_allows_mlx(Some(16 * 1024 * 1024 * 1024)));
    }

    #[test]
    fn unknown_ram_defaults_to_allowed() {
        assert!(ram_allows_mlx(None));
    }
}
