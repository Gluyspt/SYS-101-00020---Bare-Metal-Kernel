use core::cmp::Ordering;

use crate::{
    drivers::timer::Instant,
    process::{TaskDescriptor, TaskState},
};
use alloc::{boxed::Box, collections::VecDeque};
use log::warn;

use super::sched_task::SchedulableTask;

/// The result of a requested task switch.
pub enum SwitchResult {
    /// The requested task is already running. No changes made.
    AlreadyRunning,
    /// A switch occurred. The previous task was Runnable and has been
    /// re-queued.
    Preempted,
    /// A switch occurred. The previous task is Blocked (or Finished) and
    /// ownership is returned to the caller (to handle sleep/wait queues).
    Blocked { old_task: Box<SchedulableTask> },
}

/// A simple weight-tracking runqueue.
///
/// Invariants:
/// 1. `total_weight` = Sum(queue tasks) + Weight(running_task) (excluding the idle task).
/// 2. `running_task` is NOT in `queue`.
pub struct RunQueue {
    total_weight: u64,
    pub(super) queue: VecDeque<Box<SchedulableTask>>,  // changed from BTreeMap. Just do a queue and swap when time out, push to the back if not done
    pub(super) running_task: Option<Box<SchedulableTask>>,
}

impl RunQueue {
    pub const fn new() -> Self {
        Self {
            total_weight: 0,
            queue: VecDeque::new(),
            running_task: None,
        }
    }

    pub fn switch_tasks(&mut self, next_task: TaskDescriptor, now_inst: Instant) -> SwitchResult {
        if let Some(current) = self.current()
            && current.descriptor() == next_task
        {
            return SwitchResult::AlreadyRunning;
        }
        
        // Pull the chosen task out of the VecDeque
        let pos = self.queue.iter().position(|t| t.descriptor() == next_task);
        let mut new_task = match pos.and_then(|i| self.queue.remove(i)) {
            Some(t) => t,
            None => {
                warn!("Task {next_task:?} not found for switch.");
                return SwitchResult::AlreadyRunning;
            }
        };

        new_task.about_to_execute(now_inst);

        // Perform the swap.
        if let Some(old_task) = self.running_task.replace(new_task) {
            let state = *old_task.state.lock_save_irq();

            match state {
                TaskState::Running | TaskState::Runnable => {
                    // Update state to strictly Runnable
                    *old_task.state.lock_save_irq() = TaskState::Runnable;

                    self.queue.push_back(old_task); // go to back of queue

                    return SwitchResult::Preempted;
                }
                _ => {
                    self.total_weight = self.total_weight.saturating_sub(old_task.weight() as u64);

                    return SwitchResult::Blocked { old_task };
                }
            }
        }

        // If there was no previous task (e.g., boot up), it counts as a
        // Preemption.
        SwitchResult::Preempted
    }

    pub fn weight(&self) -> u64 {
        self.total_weight
    }

    #[allow(clippy::borrowed_box)]
    pub fn current(&self) -> Option<&Box<SchedulableTask>> {
        self.running_task.as_ref()
    }

    pub fn current_mut(&mut self) -> Option<&mut Box<SchedulableTask>> {
        self.running_task.as_mut()
    }

    fn fallback_current_or_idle(&self) -> TaskDescriptor {
        if let Some(ref current) = self.running_task {
            let s = *current.state.lock_save_irq();
            if !current.is_idle_task() && (s == TaskState::Runnable || s == TaskState::Running) {
                return current.descriptor();
            }
        }

        TaskDescriptor::this_cpus_idle()
    }

    /// just pick the first task in queue order.
    pub fn find_next_runnable_desc(&self) -> TaskDescriptor {
        for task in &self.queue {
            let state = *task.state.lock_save_irq();
            if !task.is_idle_task() && state == TaskState::Runnable {
                return task.descriptor();
            }
        }
        self.fallback_current_or_idle()
    }


    /// Inserts `task` into this CPU's run-queue.
    pub fn enqueue_task(&mut self, new_task: Box<SchedulableTask>) {
        if !new_task.is_idle_task() {
            self.total_weight = self.total_weight.saturating_add(new_task.weight() as u64);
        }

        self.queue.push_back(new_task); // changed from BTreeMap::insert
    }
}
