//! Poll event descriptors up to a monotonic deadline.
use std::{
    io,
    time::{Duration, Instant},
};

// Preserve the previous poll timeout bound, also fitting ARM32 timespec.tv_sec.
const MAX_WAIT: Duration = Duration::from_millis(i32::MAX as u64);

pub(crate) fn wait(
    fds: &mut [libc::pollfd],
    timeout: Option<Duration>,
    timer_quantum: Duration,
) -> io::Result<()> {
    wait_with(timeout, timer_quantum, Instant::now, |timeout| {
        let time = timeout.map(|d| libc::timespec {
            tv_sec: d.as_secs() as libc::time_t,
            tv_nsec: d.subsec_nanos() as libc::c_long,
        });
        // SAFETY: fds and optional native timespec are alive through ppoll;
        // a null signal mask preserves the calling thread's current mask.
        let result = unsafe {
            libc::ppoll(
                fds.as_mut_ptr(),
                fds.len() as libc::nfds_t,
                time.as_ref().map_or(std::ptr::null(), |t| t),
                std::ptr::null(),
            )
        };
        if result < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(result > 0)
        }
    })
}

fn wait_with(
    timeout: Option<Duration>,
    timer_quantum: Duration,
    now: impl Fn() -> Instant,
    mut poll: impl FnMut(Option<Duration>) -> io::Result<bool>,
) -> io::Result<()> {
    let timeout = timeout.map(|d| d.min(MAX_WAIT));
    if timer_quantum.is_zero() || timeout.is_none() || timeout == Some(Duration::ZERO) {
        poll(timeout)?;
        return Ok(());
    }
    let deadline = now() + timeout.unwrap();
    loop {
        let remaining = deadline.saturating_duration_since(now());
        // A coarse kernel may round a blocking timeout up by one tick. Sleep
        // until that final tick, then keep checking descriptors and the real
        // deadline. Event delivery is never delayed by the precision tail.
        if poll(Some(remaining.saturating_sub(timer_quantum)))? || remaining.is_zero() {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn coarse_kernel_wait_does_not_extend_frame_deadlines_to_the_next_tick() {
        for phase in [0, 3000, 9000] {
            for us in [1, 500, 1000, 8000, 13000, 16667, 33334, 100000] {
                let origin = Instant::now();
                let time = Cell::new(phase);
                wait_with(
                    Some(Duration::from_micros(us)),
                    Duration::from_millis(10),
                    || origin + Duration::from_micros(time.get()),
                    |timeout| {
                        let delay = timeout.unwrap().as_micros() as u64;
                        time.set(if delay == 0 {
                            time.get() + 50
                        } else {
                            (time.get() + delay).div_ceil(10_000) * 10_000
                        });
                        Ok(false)
                    },
                )
                .unwrap();
                assert!(time.get() >= phase + us);
                assert!(
                    time.get() < phase + us + 100,
                    "phase={phase} requested={us} elapsed={}",
                    time.get() - phase
                );
            }
        }
    }

    #[test]
    fn input_or_completion_interrupts_both_the_sleep_and_the_short_tail() {
        for event_at in [5_000, 32_000] {
            let origin = Instant::now();
            let time = Cell::new(0u64);
            wait_with(
                Some(Duration::from_micros(33334)),
                Duration::from_millis(10),
                || origin + Duration::from_micros(time.get()),
                |timeout| {
                    let delay = timeout.unwrap().as_micros() as u64;
                    let until = if delay == 0 {
                        time.get() + 50
                    } else {
                        (time.get() + delay).div_ceil(10_000) * 10_000
                    };
                    time.set(until.min(event_at));
                    Ok(time.get() == event_at)
                },
            )
            .unwrap();
            assert_eq!(time.get(), event_at);
        }
    }

    #[test]
    fn idle_and_default_waits_do_not_poll_repeatedly() {
        for (timeout, quantum) in [
            (None, Duration::from_millis(10)),
            (Some(Duration::ZERO), Duration::from_millis(10)),
            (Some(Duration::from_millis(33)), Duration::ZERO),
        ] {
            let mut calls = 0;
            wait_with(timeout, quantum, Instant::now, |received| {
                calls += 1;
                assert_eq!(received, timeout);
                Ok(timeout.is_none())
            })
            .unwrap();
            assert_eq!(calls, 1);
        }
        wait(&mut [], Some(Duration::ZERO), Duration::ZERO).unwrap();
    }

    #[test]
    fn wait_bounds_long_timeouts_and_returns_system_errors() {
        let error = wait_with(
            Some(Duration::MAX),
            Duration::from_millis(10),
            Instant::now,
            |timeout| {
                assert!(timeout.unwrap() <= MAX_WAIT);
                Err(io::ErrorKind::Interrupted.into())
            },
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    }
}
