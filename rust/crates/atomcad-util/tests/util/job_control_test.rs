use atomcad_util::job_control::{JobControl, JobProgress};
use std::sync::Arc;
use std::thread;

#[test]
fn a_fresh_control_is_indeterminate_and_not_cancelled() {
    let control = JobControl::new();
    assert_eq!(
        control.snapshot(),
        JobProgress {
            done: 0,
            total: None,
            phase: String::new(),
        }
    );
    assert!(!control.is_cancelled());
}

#[test]
fn a_zero_total_stays_indeterminate() {
    let control = JobControl::new();
    control.set_total(0);
    assert_eq!(control.snapshot().total, None);
    control.set_total(7);
    assert_eq!(control.snapshot().total, Some(7));
}

#[test]
fn snapshot_reports_phase_and_progress() {
    let control = JobControl::new();
    control.set_phase("Planning");
    assert_eq!(control.snapshot().phase, "Planning");
    control.set_phase("Relaxing");
    control.set_total(10);
    control.advance(3);
    assert_eq!(
        control.snapshot(),
        JobProgress {
            done: 3,
            total: Some(10),
            phase: "Relaxing".to_string(),
        }
    );
}

#[test]
fn advance_from_many_threads_sums() {
    const THREADS: u64 = 8;
    const CALLS: u64 = 1000;
    let control = Arc::new(JobControl::new());
    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let control = Arc::clone(&control);
            thread::spawn(move || {
                for _ in 0..CALLS {
                    control.advance(1);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(control.snapshot().done, THREADS * CALLS);
}

#[test]
fn cancel_is_sticky() {
    let control = JobControl::new();
    control.cancel();
    assert!(control.is_cancelled());
    control.advance(1);
    control.set_phase("Relaxing");
    control.cancel();
    assert!(control.is_cancelled());
}
