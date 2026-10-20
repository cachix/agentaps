//! Keep temporary worker allocations from inflating the desktop's resident memory.

pub(crate) fn configure_allocator() {
    // glibc raises its trim threshold after freeing large mmap allocations. With
    // many worker arenas, this can leave gigabytes in free arena tails, which
    // malloc_trim does not reclaim outside the main arena. Fix the threshold at
    // glibc's initial 128 KiB so normal frees return those tails to the OS.
    // SAFETY: called at the start of main, before application workers start.
    // This changes reclamation policy without invalidating live allocations.
    unsafe {
        libc::mallopt(libc::M_TRIM_THRESHOLD, 128 * 1024);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        process::Command,
        sync::{Arc, Barrier},
    };

    const CHILD: &str = "AGENTAPS_ALLOCATOR_TEST";

    #[test]
    fn reclaims_worker_allocations() {
        if let Ok(mode) = std::env::var(CHILD) {
            if mode == "fixed" {
                super::configure_allocator();
            }
            // A large temporary allocation raises glibc's adaptive thresholds.
            unsafe {
                let warmup = libc::malloc(16 * 1024 * 1024);
                assert!(!warmup.is_null());
                std::hint::black_box(warmup);
                libc::free(warmup);
            }
            let ready = Arc::new(Barrier::new(17));
            let release = Arc::new(Barrier::new(17));
            let workers: Vec<_> = (0..16)
                .map(|_| {
                    let ready = ready.clone();
                    let release = release.clone();
                    std::thread::spawn(move || {
                        // Keep the arena attached while measuring its free tail.
                        // SAFETY: each allocation is checked, initialized within its
                        // bounds, and freed once by the allocating thread.
                        unsafe {
                            let anchor = libc::malloc(64);
                            let buffer = libc::malloc(8 * 1024 * 1024);
                            assert!(!anchor.is_null() && !buffer.is_null());
                            std::ptr::write_bytes(buffer.cast::<u8>(), 1, 8 * 1024 * 1024);
                            std::hint::black_box(buffer);
                            ready.wait();
                            libc::free(buffer);
                            ready.wait();
                            release.wait();
                            libc::free(anchor);
                        }
                    })
                })
                .collect();
            ready.wait();
            ready.wait();
            // SAFETY: mallinfo2 only reads allocator statistics.
            println!("retained_bytes={}", unsafe { libc::mallinfo2().fordblks });
            release.wait();
            for worker in workers {
                worker.join().unwrap();
            }
            return;
        }

        let retained = |mode| {
            let output = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "memory::tests::reclaims_worker_allocations",
                    "--nocapture",
                ])
                .env(CHILD, mode)
                .env_remove("GLIBC_TUNABLES")
                .env_remove("MALLOC_TRIM_THRESHOLD_")
                .env_remove("MALLOC_MMAP_THRESHOLD_")
                .env_remove("MALLOC_ARENA_MAX")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("retained_bytes="))
                .unwrap()
                .parse::<usize>()
                .unwrap()
        };
        let baseline = retained("default");
        let fixed = retained("fixed");
        assert!(
            baseline > fixed + 64 * 1024 * 1024,
            "worker arenas retained {baseline} bytes by default versus {fixed} with the fixed trim threshold"
        );
    }
}
