#![deny(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]

use rusty_termcolor::{
    colors::{GREEN, MAGENTA},
    effects::{EffectSettings, typewriter},
    formatting::println_colored,
};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tokio::time::sleep;

type TaskId = u32;
type TaskResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

pub struct Task {
    id: TaskId,
    name: String,
    priority: u8,
    dependencies: Vec<TaskId>,
    function: Box<dyn Fn() -> TaskResult + Send + Sync>,
}

impl Task {
    pub fn new<F>(id: TaskId, name: String, priority: u8, function: F) -> Self
    where
        F: Fn() -> TaskResult + Send + Sync + 'static,
    {
        Self {
            id,
            name,
            priority,
            dependencies: Vec::new(),
            function: Box::new(function),
        }
    }

    #[must_use]
    pub fn with_dependencies(mut self, dependencies: Vec<TaskId>) -> Self {
        self.dependencies = dependencies;
        self
    }

    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.id
    }

    #[must_use]
    pub const fn priority(&self) -> u8 {
        self.priority
    }

    #[must_use]
    pub fn dependencies(&self) -> &[TaskId] {
        &self.dependencies
    }
}

// TODO: try to make SharedState copy-able
#[derive(Clone)]
struct SharedState {
    task_queue: Arc<Mutex<VecDeque<Task>>>,
    task_status: Arc<Mutex<HashMap<TaskId, TaskStatus>>>,
    completed_tasks: Arc<Mutex<Vec<TaskId>>>,
    condvar: Arc<Condvar>,
    shutdown: Arc<Mutex<bool>>,
}

// TODO: try to make Scheduler copy-able
#[derive(Clone)]
pub struct Scheduler {
    shared_state: SharedState,
    thread_count: usize,
}

impl Scheduler {
    #[must_use]
    pub fn new(thread_count: usize) -> Self {
        let shared_state = SharedState {
            task_queue: Arc::new(Mutex::new(VecDeque::new())),
            task_status: Arc::new(Mutex::new(HashMap::new())),
            completed_tasks: Arc::new(Mutex::new(Vec::new())),
            condvar: Arc::new(Condvar::new()),
            shutdown: Arc::new(Mutex::new(false)),
        };

        Self {
            shared_state,
            thread_count,
        }
    }

    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn add_task(&self, task: Task) {
        let task_id = task.id();

        {
            let mut status = self.shared_state.task_status.lock().unwrap();
            status.insert(task_id, TaskStatus::Pending);
        }

        {
            let mut queue = self.shared_state.task_queue.lock().unwrap();
            // Insert task maintaining priority order (higher priority first)
            let insert_pos = queue
                .iter()
                .position(|t| t.priority() < task.priority())
                .unwrap_or(queue.len());
            queue.insert(insert_pos, task);
        }

        self.shared_state.condvar.notify_one();
    }

    #[must_use]
    pub fn start(self) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();

        for thread_id in 0..self.thread_count {
            let state = self.shared_state.clone();

            let handle = thread::spawn(move || {
                Self::worker_thread(thread_id, &state);
            });

            handles.push(handle);
        }

        handles
    }

    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub fn shutdown(&self) {
        {
            let mut shutdown = self.shared_state.shutdown.lock().unwrap();
            *shutdown = true;
        }
        self.shared_state.condvar.notify_all();
    }

    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    #[must_use]
    pub fn get_task_status(&self, task_id: TaskId) -> Option<TaskStatus> {
        let status = self.shared_state.task_status.lock().unwrap();
        status.get(&task_id).cloned()
    }

    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    #[must_use]
    pub fn get_all_statuses(&self) -> HashMap<TaskId, TaskStatus> {
        let status = self.shared_state.task_status.lock().unwrap();
        status.clone()
    }

    /// # Panics
    ///
    /// Panics if a mutex lock is poisoned.
    pub async fn wait_for_all_tasks_to_complete(&self) {
        loop {
            let all_statuses = self.get_all_statuses();
            let all_completed = all_statuses
                .values()
                .all(|status| matches!(status, TaskStatus::Completed | TaskStatus::Failed));

            if all_completed {
                break;
            }

            sleep(Duration::from_millis(100)).await;
        }
    }

    fn worker_thread(thread_id: usize, state: &SharedState) {
        loop {
            let task = {
                let mut queue = state.task_queue.lock().unwrap();

                loop {
                    let shutdown = *state.shutdown.lock().unwrap();
                    if shutdown {
                        return;
                    }

                    if let Some(task) = Self::find_ready_task(&mut queue, state) {
                        break Some(task);
                    }

                    queue = state
                        .condvar
                        .wait_timeout(queue, Duration::from_millis(100))
                        .unwrap()
                        .0;
                }
            };

            if let Some(task) = task {
                println!(
                    "ℹ️ Thread {} executing task: {} (ID: {})",
                    thread_id,
                    task.name,
                    task.id()
                );

                // Mark task as running
                {
                    let mut status = state.task_status.lock().unwrap();
                    status.insert(task.id(), TaskStatus::Running);
                }

                // Execute task with panic handling
                let task_id = task.id();
                let result =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (task.function)()));

                // Update task status based on result
                match result {
                    Ok(Ok(())) => {
                        {
                            let mut status = state.task_status.lock().unwrap();
                            status.insert(task_id, TaskStatus::Completed);
                        }
                        state.completed_tasks.lock().unwrap().push(task_id);
                        println!("✅ Thread {thread_id} completed task ID: {task_id}");
                    }
                    Ok(Err(e)) => {
                        let mut status = state.task_status.lock().unwrap();
                        status.insert(task_id, TaskStatus::Failed);
                        drop(status);
                        eprintln!("🚨 Thread {thread_id} - Task {task_id} failed with error: {e}");
                    }
                    Err(_) => {
                        let mut status = state.task_status.lock().unwrap();
                        status.insert(task_id, TaskStatus::Failed);
                        drop(status);
                        eprintln!("🚨 Thread {thread_id} - Task {task_id} panicked");
                    }
                }

                // Notify other threads that a task completed (might unblock dependencies)
                state.condvar.notify_all();
            }
        }
    }

    fn find_ready_task(queue: &mut VecDeque<Task>, state: &SharedState) -> Option<Task> {
        let completed_tasks = state.completed_tasks.lock().unwrap();

        for i in 0..queue.len() {
            let task = &queue[i];

            // Check if all dependencies are completed
            let dependencies_met = task
                .dependencies()
                .iter()
                .all(|dep_id| completed_tasks.contains(dep_id));

            if dependencies_met {
                println!("🟢 All dependencies met for {}", task.id());
                return queue.remove(i);
            }
        }

        None
    }
}

fn type_write(value: &str) {
    typewriter(
        &format!("\n 🎺 {value:?} 🎺\n"),
        &EffectSettings::default(),
        Some(&GREEN),
    );
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    type_write("Starting the scheduler demo!");

    let scheduler = Scheduler::new(2);

    let task_1 = Task::new(1, "Task 1".to_string(), 1, || {
        std::thread::sleep(Duration::from_secs(10));
        println!("👋 Hi from Task 1");
        Ok(())
    })
    .with_dependencies(vec![]);

    let task_2 = Task::new(2, "Task 2".to_string(), 2, || {
        println!("👋 Hi from Task 2");
        Ok(())
    })
    .with_dependencies(vec![]);

    let task_3 = Task::new(3, "Task 3".to_string(), 3, || {
        println!("👋 Hi from Task 3");
        Ok(())
    })
    .with_dependencies(vec![1]);

    let task_4 = Task::new(4, "Task 4".to_string(), 4, || {
        println_colored("👋 Hi from the tainted Task 4", &MAGENTA);
        panic!("😱 Task 4 is panicking! 😱");
    })
    .with_dependencies(vec![3, 2]);

    scheduler.add_task(task_1);
    scheduler.add_task(task_2);
    scheduler.add_task(task_3);
    scheduler.add_task(task_4);

    // Create a clone of the shared state before moving scheduler
    let scheduler_clone = scheduler.clone();
    let handles = scheduler.start();

    scheduler_clone.wait_for_all_tasks_to_complete().await;
    scheduler_clone.shutdown();

    // Wait for all worker threads to finish
    for handle in handles {
        handle.join().expect("Worker thread panicked");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::Instant;

    #[test]
    fn test_task_creation() {
        let task = Task::new(1, "Test Task".to_string(), 5, || Ok(()));
        assert_eq!(task.id(), 1);
        assert_eq!(task.priority(), 5);
        assert!(task.dependencies().is_empty());
    }

    #[test]
    fn test_task_with_dependencies() {
        let task =
            Task::new(1, "Test Task".to_string(), 5, || Ok(())).with_dependencies(vec![2, 3]);
        assert_eq!(task.dependencies(), &[2, 3]);
    }

    #[test]
    fn test_scheduler_creation() {
        let scheduler = Scheduler::new(4);
        assert_eq!(scheduler.thread_count, 4);
    }

    #[test]
    fn test_task_status_tracking() {
        let scheduler = Scheduler::new(1);

        let task = Task::new(1, "Test Task".to_string(), 1, || Ok(()));
        scheduler.add_task(task);

        assert_eq!(scheduler.get_task_status(1), Some(TaskStatus::Pending));
        assert_eq!(scheduler.get_task_status(999), None);
    }

    #[test]
    fn test_simple_task_execution() {
        let scheduler = Scheduler::new(1);
        let executed = Arc::new(AtomicBool::new(false));
        let executed_clone = executed.clone();

        let task = Task::new(1, "Test Task".to_string(), 1, move || {
            executed_clone.store(true, Ordering::SeqCst);
            Ok(())
        });

        scheduler.add_task(task);

        // Create a clone of the scheduler before moving it
        let scheduler_clone = scheduler.clone();
        let handles = scheduler.start();

        // Wait for task to complete
        let start = Instant::now();
        while !executed.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }

        scheduler_clone.shutdown();

        for handle in handles {
            handle.join().unwrap();
        }

        assert!(executed.load(Ordering::SeqCst));
        assert_eq!(
            scheduler_clone.get_task_status(1),
            Some(TaskStatus::Completed)
        );
    }

    #[test]
    fn test_task_priority_ordering() {
        let scheduler = Scheduler::new(1);
        let execution_order = Arc::new(Mutex::new(Vec::new()));

        let order_clone1 = execution_order.clone();
        let task1 = Task::new(1, "Low Priority".to_string(), 1, move || {
            order_clone1.lock().unwrap().push(1);
            Ok(())
        });

        let order_clone2 = execution_order.clone();
        let task2 = Task::new(2, "High Priority".to_string(), 10, move || {
            order_clone2.lock().unwrap().push(2);
            Ok(())
        });

        // Add low priority first, then high priority
        scheduler.add_task(task1);
        scheduler.add_task(task2);

        // Create a clone of the scheduler before moving it
        let scheduler_clone = scheduler.clone();
        let handles = scheduler.start();

        // Wait for tasks to complete
        let start = Instant::now();
        while execution_order.lock().unwrap().len() < 2 && start.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
        }

        scheduler_clone.shutdown();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(*execution_order.lock().unwrap(), vec![2, 1]); // High priority task should execute first
    }

    #[test]
    fn test_task_dependencies() {
        let scheduler = Scheduler::new(2);
        let execution_order = Arc::new(Mutex::new(Vec::new()));

        let order_clone1 = execution_order.clone();
        let task1 = Task::new(1, "Independent Task".to_string(), 1, move || {
            order_clone1.lock().unwrap().push(1);
            Ok(())
        });

        let order_clone2 = execution_order.clone();
        let task2 = Task::new(2, "Dependent Task".to_string(), 10, move || {
            order_clone2.lock().unwrap().push(2);
            Ok(())
        })
        .with_dependencies(vec![1]);

        scheduler.add_task(task2); // Add dependent task first
        scheduler.add_task(task1); // Add dependency second

        // Create a clone of the scheduler before moving it
        let scheduler_clone = scheduler.clone();
        let handles = scheduler.start();

        // Wait for tasks to complete
        let start = Instant::now();
        while execution_order.lock().unwrap().len() < 2 && start.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
        }

        scheduler_clone.shutdown();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(*execution_order.lock().unwrap(), vec![1, 2]); // Task 1 must execute before Task 2
    }

    #[test]
    fn test_task_failure_handling() {
        let scheduler = Scheduler::new(1);

        let task = Task::new(1, "Failing Task".to_string(), 1, || {
            Err("Task failed".into())
        });

        scheduler.add_task(task);

        // Create a clone of the scheduler before moving it
        let scheduler_clone = scheduler.clone();
        let handles = scheduler.start();

        // Wait for task to fail
        let start = Instant::now();
        while scheduler_clone.get_task_status(1) != Some(TaskStatus::Failed)
            && start.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
        }

        scheduler_clone.shutdown();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(scheduler_clone.get_task_status(1), Some(TaskStatus::Failed));
    }

    #[test]
    fn test_task_panic_handling() {
        let scheduler = Scheduler::new(1);

        let task = Task::new(1, "Panicking Task".to_string(), 1, || {
            panic!("Task panicked");
        });

        scheduler.add_task(task);

        // Create a clone of the scheduler before moving it
        let scheduler_clone = scheduler.clone();
        let handles = scheduler.start();

        // Wait for task to fail due to panic
        let start = Instant::now();
        while scheduler_clone.get_task_status(1) != Some(TaskStatus::Failed)
            && start.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
        }

        scheduler_clone.shutdown();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(scheduler_clone.get_task_status(1), Some(TaskStatus::Failed));
    }

    #[test]
    fn test_multiple_thread_execution() {
        let scheduler = Scheduler::new(3);
        let counter = Arc::new(AtomicU32::new(0));

        for i in 1..=5 {
            let counter_clone = counter.clone();
            let task = Task::new(i, format!("Task {i}"), 1, move || {
                counter_clone.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(100));
                Ok(())
            });
            scheduler.add_task(task);
        }

        // Create a clone of the scheduler before moving it
        let scheduler_clone = scheduler.clone();
        let handles = scheduler.start();

        // Wait for all tasks to complete
        let start = Instant::now();
        while counter.load(Ordering::SeqCst) < 5 && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }

        scheduler_clone.shutdown();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(counter.load(Ordering::SeqCst), 5);

        // Check all tasks completed
        let statuses = scheduler_clone.get_all_statuses();
        for i in 1..=5 {
            assert_eq!(statuses.get(&i), Some(&TaskStatus::Completed));
        }
    }

    #[test]
    fn test_get_all_statuses() {
        let scheduler = Scheduler::new(1);

        let task1 = Task::new(1, "Task 1".to_string(), 1, || Ok(()));
        let task2 = Task::new(2, "Task 2".to_string(), 2, || Ok(()));

        scheduler.add_task(task1);
        scheduler.add_task(task2);

        let statuses = scheduler.get_all_statuses();
        assert_eq!(statuses.len(), 2);
        assert_eq!(statuses.get(&1), Some(&TaskStatus::Pending));
        assert_eq!(statuses.get(&2), Some(&TaskStatus::Pending));
    }

    #[test]
    fn test_complex_dependency_chain() {
        let scheduler = Scheduler::new(2);
        let execution_order = Arc::new(Mutex::new(Vec::new()));

        // Create a chain: Task 1 -> Task 2 -> Task 3
        let order_clone1 = execution_order.clone();
        let task1 = Task::new(1, "Task 1".to_string(), 1, move || {
            order_clone1.lock().unwrap().push(1);
            std::thread::sleep(Duration::from_millis(50));
            Ok(())
        });

        let order_clone2 = execution_order.clone();
        let task2 = Task::new(2, "Task 2".to_string(), 1, move || {
            order_clone2.lock().unwrap().push(2);
            std::thread::sleep(Duration::from_millis(50));
            Ok(())
        })
        .with_dependencies(vec![1]);

        let order_clone3 = execution_order.clone();
        let task3 = Task::new(3, "Task 3".to_string(), 1, move || {
            order_clone3.lock().unwrap().push(3);
            Ok(())
        })
        .with_dependencies(vec![2]);

        // Add tasks in reverse order
        scheduler.add_task(task3);
        scheduler.add_task(task2);
        scheduler.add_task(task1);

        // Create a clone of the scheduler before moving it
        let scheduler_clone = scheduler.clone();
        let handles = scheduler.start();

        // Wait for all tasks to complete
        let start = Instant::now();
        while execution_order.lock().unwrap().len() < 3 && start.elapsed() < Duration::from_secs(5)
        {
            std::thread::sleep(Duration::from_millis(10));
        }

        scheduler_clone.shutdown();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(*execution_order.lock().unwrap(), vec![1, 2, 3]);
    }
}
