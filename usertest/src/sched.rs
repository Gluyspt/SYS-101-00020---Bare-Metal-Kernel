use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

struct TaskResult {
    id: usize,
    wait_ms: u128,
    completion_ms: u128,
}

pub fn test_scheduler_benchmark() {
    const NUM_TASKS: usize = 5;
    let workloads: [usize; NUM_TASKS] = [300_000, 100_000, 400_000, 100_000, 200_000];

    // Barrier ensures all threads are spawned before any starts working
    // This makes the comparison fair between schedulers
    let barrier = Arc::new(Barrier::new(NUM_TASKS));
    let mut handles = Vec::with_capacity(NUM_TASKS);

    for i in 0..NUM_TASKS {
        let barrier = Arc::clone(&barrier);
        let work = workloads[i];

        handles.push(thread::spawn(move || {
            let arrival = Instant::now();

            // All tasks released simultaneously
            barrier.wait();

            let first_run = Instant::now();

            // CPU-bound work — gives scheduler real work to preempt
            let mut x: u64 = 0;
            for j in 0..work as u64 {
                x = x.wrapping_add(j);
            }
            // Prevent optimizer from eliminating the loop
            assert!(x < u64::MAX);

            let finish = Instant::now();

            TaskResult {
                id: i,
                wait_ms: (first_run - arrival).as_millis(),
                completion_ms: (finish - arrival).as_millis(),
            }
        }));
    }

    println!("\n--- Scheduler Benchmark Results ---");
    println!("{:<8} {:<12} {:<16}", "Task", "Wait (ms)", "Completion (ms)");
    println!("{}", "-".repeat(38));

    let mut total_wait = 0u128;
    let mut total_completion = 0u128;

    for handle in handles {
        let r = handle.join().unwrap();
        println!(
            "{:<8} {:<12} {:<16}",
            r.id, r.wait_ms, r.completion_ms
        );
        total_wait += r.wait_ms;
        total_completion += r.completion_ms;
    }

    println!("{}", "-".repeat(38));
    println!(
        "{:<8} {:<12} {:<16}",
        "AVG",
        total_wait / NUM_TASKS as u128,
        total_completion / NUM_TASKS as u128
    );
    println!("-----------------------------------\n");
}